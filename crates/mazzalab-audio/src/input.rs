//! Audio inputs: listing them, and reading linear timecode from a channel
//! of one. The input stream only copies samples into a ring buffer; a
//! thread of its own reads the timecode out of them, so the driver's
//! callback never waits.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, StreamConfig};
use mazzalab_timecode::ltc::Reader;
use mazzalab_timecode::{Rate, Timecode};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::AudioError;

/// How often the reading thread looks for new samples.
const POLL: Duration = Duration::from_millis(5);

/// An input device a signal can come in on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    /// Stable identifier, stored in the show's settings.
    pub id: String,
    pub name: String,
    /// Audio system the device belongs to.
    pub host: String,
    pub is_default: bool,
    /// Most input channels any of its formats offers.
    pub max_channels: u16,
}

/// Every input device of every audio system on this machine.
pub fn input_devices() -> Vec<InputDevice> {
    let mut devices = Vec::new();
    for host_id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        let default = host
            .default_input_device()
            .and_then(|device| device.id().ok());
        let Ok(inputs) = host.input_devices() else {
            continue;
        };
        for device in inputs {
            let Ok(id) = device.id() else {
                continue;
            };
            let max_channels = device
                .supported_input_configs()
                .map(|configs| configs.map(|config| config.channels()).max().unwrap_or(0))
                .unwrap_or(0);
            devices.push(InputDevice {
                name: device
                    .description()
                    .map(|description| description.name().to_owned())
                    .unwrap_or_else(|_| device.to_string()),
                host: host_id.to_string(),
                is_default: default.as_ref() == Some(&id),
                max_channels,
                id: id.to_string(),
            });
        }
    }
    devices
}

/// What an [`LtcInput`] listens to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LtcInputInfo {
    pub name: String,
    pub sample_rate: u32,
    /// The channel read, from 0.
    pub channel: usize,
    /// The requested device was not found and the default input is in use.
    pub fallback: bool,
}

/// Reads LTC from one channel of an input device and hands every frame,
/// with its rate, to a callback. Dropping it stops listening.
pub struct LtcInput {
    info: LtcInputInfo,
    stop: Arc<AtomicBool>,
    stream: Option<mpsc::Sender<()>>,
    threads: Vec<JoinHandle<()>>,
}

impl LtcInput {
    /// Starts listening on `device` (an [`InputDevice::id`]; `None` is the
    /// system default input), `channel` counted from 0.
    pub fn start(
        device: Option<&str>,
        channel: usize,
        on_frame: impl FnMut(Timecode, Rate) + Send + 'static,
    ) -> Result<Self, AudioError> {
        let device_id = device.map(str::to_owned);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        // cpal streams are not `Send` everywhere: the stream lives and dies
        // on a thread of its own.
        let stream_thread = std::thread::Builder::new()
            .name("mazzalab-ltc-input".into())
            .spawn(move || match open(device_id.as_deref(), channel) {
                Ok((info, stream, consumer)) => {
                    let _ = ready_tx.send(Ok((info, consumer)));
                    let _ = shutdown_rx.recv();
                    drop(stream);
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .map_err(|error| AudioError::Device(error.to_string()))?;
        let (info, consumer) = ready_rx
            .recv()
            .map_err(|_| AudioError::Device("audio input thread exited".into()))??;

        let stop = Arc::new(AtomicBool::new(false));
        let reading = Arc::clone(&stop);
        let sample_rate = info.sample_rate;
        let reader_thread = std::thread::Builder::new()
            .name("mazzalab-ltc-reader".into())
            .spawn(move || read(consumer, sample_rate, &reading, on_frame))
            .map_err(|error| AudioError::Device(error.to_string()))?;
        Ok(Self {
            info,
            stop,
            stream: Some(shutdown_tx),
            threads: vec![stream_thread, reader_thread],
        })
    }

    pub fn info(&self) -> &LtcInputInfo {
        &self.info
    }
}

impl Drop for LtcInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.stream.take());
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl std::fmt::Debug for LtcInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LtcInput")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

/// Reads timecode out of the samples the stream leaves, until told to stop.
fn read(
    mut consumer: Consumer<f32>,
    sample_rate: u32,
    stop: &AtomicBool,
    mut on_frame: impl FnMut(Timecode, Rate),
) {
    let mut reader = Reader::new(sample_rate);
    let mut block = Vec::with_capacity(4096);
    while !stop.load(Ordering::Relaxed) {
        block.clear();
        while let Ok(sample) = consumer.pop() {
            block.push(sample);
            if block.len() == block.capacity() {
                break;
            }
        }
        if block.is_empty() {
            std::thread::sleep(POLL);
            continue;
        }
        for (timecode, rate) in reader.feed(&block) {
            on_frame(timecode, rate);
        }
    }
}

fn open(
    device_id: Option<&str>,
    channel: usize,
) -> Result<(LtcInputInfo, cpal::Stream, Consumer<f32>), AudioError> {
    let error = |error: &dyn std::fmt::Display| AudioError::Device(error.to_string());
    let wanted = device_id.and_then(|id| {
        let id = cpal::DeviceId::from_str(id).ok()?;
        cpal::host_from_id(id.host()).ok()?.device_by_id(&id)
    });
    let fallback = device_id.is_some() && wanted.is_none();
    let device = match wanted {
        Some(device) => device,
        None => cpal::default_host()
            .default_input_device()
            .ok_or_else(|| AudioError::Device("no input device".into()))?,
    };
    let supported = device.default_input_config().map_err(|e| error(&e))?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let channels = usize::from(config.channels).max(1);
    let channel = channel.min(channels - 1);
    // Half a second of samples is plenty for a reader that looks every 5 ms.
    let (producer, consumer) = RingBuffer::new(config.sample_rate as usize / 2);
    let info = LtcInputInfo {
        name: device
            .description()
            .map(|description| description.name().to_owned())
            .unwrap_or_else(|_| device.to_string()),
        sample_rate: config.sample_rate,
        channel,
        fallback,
    };
    let stream = match sample_format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, channel, producer),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, channel, producer),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, channel, producer),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, channel, producer),
        other => Err(AudioError::Device(format!(
            "unsupported input sample format {other}"
        ))),
    }?;
    stream.play().map_err(|e| error(&e))?;
    Ok((info, stream, consumer))
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    channel: usize,
    mut producer: Producer<f32>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // A reader that fell behind loses samples, not the stream.
                for frame in data.chunks(channels) {
                    if let Some(sample) = frame.get(channel) {
                        let _ = producer.push(cpal::Sample::to_sample::<f32>(*sample));
                    }
                }
            },
            |error| tracing::warn!(%error, "audio input stream error"),
            None,
        )
        .map_err(|error| AudioError::Device(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazzalab_timecode::ltc::Encoder;

    #[test]
    fn the_reading_thread_hands_on_every_frame_with_its_rate() {
        let sample_rate = 48_000;
        let (mut producer, consumer) = RingBuffer::new(sample_rate as usize);
        let stop = Arc::new(AtomicBool::new(false));
        let (frames_tx, frames_rx) = mpsc::channel();
        let reading = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            read(consumer, sample_rate, &reading, move |timecode, rate| {
                let _ = frames_tx.send((timecode, rate));
            });
        });

        // What an input stream would leave: half a second of 25 fps LTC.
        let start = Timecode::new(1, 0, 0, 0);
        let mut encoder = Encoder::new(start, Rate::FPS_25, sample_rate, 0.3);
        let mut block = vec![0.0; sample_rate as usize / 2];
        encoder.fill(&mut block);
        for sample in block {
            producer.push(sample).unwrap();
        }
        let mut read_back = Vec::new();
        while let Ok(frame) = frames_rx.recv_timeout(Duration::from_secs(2)) {
            read_back.push(frame);
            if read_back.len() == 10 {
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        assert_eq!(read_back.len(), 10);
        assert!(read_back.iter().all(|(_, rate)| *rate == Rate::FPS_25));
        assert_eq!(read_back[0].0, Rate::FPS_25.add(start, 1));
    }
}

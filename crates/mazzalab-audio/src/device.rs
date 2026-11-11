//! Output device handling with cpal: listing devices, opening the chosen one
//! with the requested format, and keeping its stream alive. The stream lives
//! on its own thread because cpal streams are not `Send` on every platform.

use std::str::FromStr;
use std::sync::mpsc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig,
};

use crate::AudioError;
use crate::mixer::{MAX_OUTPUTS, Mixer};

/// Sample rates offered in device lists when the driver supports them.
const COMMON_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

/// An output device the show can play on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
    /// Stable identifier, stored in the show's audio settings.
    pub id: String,
    pub name: String,
    /// Audio system the device belongs to (WASAPI, CoreAudio, ALSA...).
    pub host: String,
    pub is_default: bool,
    /// Most output channels any of its formats offers.
    pub max_channels: u16,
    /// Common sample rates the device accepts.
    pub sample_rates: Vec<u32>,
    /// What the device runs at when nothing is requested.
    pub default_sample_rate: Option<u32>,
}

/// Which device to open and how. Every field is a wish: anything the device
/// cannot do falls back to what it can, and the [`DeviceInfo`] says what was
/// actually opened.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceRequest {
    /// An [`OutputDevice::id`]; `None` is the system default output.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    /// Buffer size in frames.
    pub buffer_size: Option<u32>,
}

/// The output the mixer renders to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: usize,
    /// Buffer size in frames, when a fixed one was accepted.
    pub buffer_size: Option<u32>,
    /// The requested device was not found and the default one is in use.
    pub fallback: bool,
}

/// Every output device of every audio system on this machine.
pub fn output_devices() -> Vec<OutputDevice> {
    let mut devices = Vec::new();
    for host_id in cpal::available_hosts() {
        let Ok(host) = cpal::host_from_id(host_id) else {
            continue;
        };
        let default = host
            .default_output_device()
            .and_then(|device| device.id().ok());
        let Ok(outputs) = host.output_devices() else {
            continue;
        };
        for device in outputs {
            let Ok(id) = device.id() else {
                continue;
            };
            let ranges: Vec<_> = device
                .supported_output_configs()
                .map(Iterator::collect)
                .unwrap_or_default();
            devices.push(OutputDevice {
                name: display_name(&device),
                host: host_id.to_string(),
                is_default: default.as_ref() == Some(&id),
                max_channels: ranges
                    .iter()
                    .map(cpal::SupportedStreamConfigRange::channels)
                    .max()
                    .unwrap_or(0),
                sample_rates: COMMON_RATES
                    .into_iter()
                    .filter(|rate| {
                        ranges.iter().any(|range| {
                            range.min_sample_rate() <= *rate && *rate <= range.max_sample_rate()
                        })
                    })
                    .collect(),
                default_sample_rate: device
                    .default_output_config()
                    .ok()
                    .map(|config| config.sample_rate()),
                id: id.to_string(),
            });
        }
    }
    devices
}

fn display_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| device.to_string())
}

/// Keeps the stream thread alive; dropping it stops the stream.
#[derive(Debug)]
pub(crate) struct DeviceThread {
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for DeviceThread {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens the requested output device and starts rendering with the mixer
/// that `make_mixer` builds for the device's channel count.
pub(crate) fn start(
    request: DeviceRequest,
    make_mixer: impl FnOnce(&DeviceInfo) -> Mixer + Send + 'static,
) -> Result<(DeviceInfo, DeviceThread), AudioError> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("mazzalab-audio-device".into())
        .spawn(move || {
            let stream = match open(&request, make_mixer) {
                Ok((info, stream)) => {
                    let _ = ready_tx.send(Ok(info));
                    stream
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            // Park until the owner goes away, then drop the stream.
            let _ = shutdown_rx.recv();
            drop(stream);
        })
        .map_err(|error| AudioError::Device(error.to_string()))?;

    let info = ready_rx
        .recv()
        .map_err(|_| AudioError::Device("audio thread exited".into()))??;
    Ok((
        info,
        DeviceThread {
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        },
    ))
}

/// The device named by `id`, looked up in the audio system it belongs to.
fn find_device(id: &str) -> Option<cpal::Device> {
    let id = cpal::DeviceId::from_str(id).ok()?;
    cpal::host_from_id(id.host()).ok()?.device_by_id(&id)
}

/// Picks the format to open: the requested sample rate with as many channels
/// as possible when the device offers it, else the device's own default.
fn choose_config(
    device: &cpal::Device,
    sample_rate: Option<u32>,
) -> Result<SupportedStreamConfig, AudioError> {
    let default = device
        .default_output_config()
        .map_err(|error| AudioError::Device(error.to_string()))?;
    let Some(rate) = sample_rate.filter(|rate| *rate != default.sample_rate()) else {
        return Ok(default);
    };
    let wanted = device
        .supported_output_configs()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|range| range.try_with_sample_rate(rate))
        // Same channel count as the default when possible, floats preferred.
        .max_by_key(|config| {
            (
                config.channels() == default.channels(),
                config.sample_format() == SampleFormat::F32,
                config.channels(),
            )
        });
    if wanted.is_none() {
        tracing::warn!(
            rate,
            "sample rate not supported by the device; using its default"
        );
    }
    Ok(wanted.unwrap_or(default))
}

fn open(
    request: &DeviceRequest,
    make_mixer: impl FnOnce(&DeviceInfo) -> Mixer,
) -> Result<(DeviceInfo, cpal::Stream), AudioError> {
    let device_error = |error: &dyn std::fmt::Display| AudioError::Device(error.to_string());
    let wanted = request.device.as_deref().and_then(find_device);
    let fallback = request.device.is_some() && wanted.is_none();
    if fallback {
        tracing::warn!(
            device = request.device.as_deref().unwrap_or_default(),
            "audio device not found; using the default output"
        );
    }
    let device = match wanted {
        Some(device) => device,
        None => cpal::default_host()
            .default_output_device()
            .ok_or_else(|| AudioError::Device("no output device".into()))?,
    };
    let supported = choose_config(&device, request.sample_rate)?;
    let sample_format = supported.sample_format();
    let mut config: StreamConfig = supported.into();
    if usize::from(config.channels) > MAX_OUTPUTS {
        return Err(AudioError::Device(format!(
            "devices with more than {MAX_OUTPUTS} channels are not supported yet"
        )));
    }
    let mut info = DeviceInfo {
        id: device
            .id()
            .map_or_else(|_| "default".to_owned(), |id| id.to_string()),
        name: display_name(&device),
        sample_rate: config.sample_rate,
        channels: usize::from(config.channels),
        buffer_size: None,
        fallback,
    };
    let mixer = make_mixer(&info);

    // A driver that refuses the requested buffer size still gets to play
    // with its own. The size is tried on a silent stream first, because a
    // failed attempt would swallow the mixer.
    if let Some(frames) = request.buffer_size.filter(|frames| *frames > 0) {
        config.buffer_size = BufferSize::Fixed(frames);
        match build_any(&device, config, sample_format, None) {
            Ok(probe) => {
                drop(probe);
                info.buffer_size = Some(frames);
            }
            Err(error) => {
                tracing::warn!(%error, frames, "buffer size refused by the device; using its default");
                config.buffer_size = BufferSize::Default;
            }
        }
    }
    let stream = build_any(&device, config, sample_format, Some(mixer))?;
    stream.play().map_err(|e| device_error(&e))?;
    Ok((info, stream))
}

/// Builds a stream in the device's sample format. Without a mixer the stream
/// plays silence (used to test a configuration).
fn build_any(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    mixer: Option<Mixer>,
) -> Result<cpal::Stream, AudioError> {
    match sample_format {
        SampleFormat::F32 => build::<f32>(device, config, mixer),
        SampleFormat::F64 => build::<f64>(device, config, mixer),
        SampleFormat::I16 => build::<i16>(device, config, mixer),
        SampleFormat::I32 => build::<i32>(device, config, mixer),
        SampleFormat::U16 => build::<u16>(device, config, mixer),
        SampleFormat::U8 => build::<u8>(device, config, mixer),
        other => Err(AudioError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut mixer: Option<Mixer>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    // Pre-allocated so the callback never allocates.
    let block = (16_384 / channels) * channels;
    let mut scratch = vec![0.0_f32; block];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for chunk in data.chunks_mut(block) {
                    let buffer = &mut scratch[..chunk.len()];
                    match &mut mixer {
                        Some(mixer) => mixer.process(buffer),
                        None => buffer.fill(0.0),
                    }
                    for (out, sample) in chunk.iter_mut().zip(buffer.iter()) {
                        *out = T::from_sample(sample.clamp(-1.0, 1.0));
                    }
                }
            },
            |error| tracing::warn!(%error, "audio stream error"),
            None,
        )
        .map_err(|error| AudioError::Device(error.to_string()))
}

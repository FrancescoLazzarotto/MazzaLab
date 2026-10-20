//! Output device handling with cpal. The stream lives on its own thread
//! because cpal streams are not `Send` on every platform.

use std::sync::mpsc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::AudioError;
use crate::mixer::Mixer;

/// The output the mixer renders to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub sample_rate: u32,
    pub channels: usize,
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

/// Opens the default output device and starts rendering with the mixer that
/// `make_mixer` builds for the device's channel count.
pub(crate) fn start(
    make_mixer: impl FnOnce(&DeviceInfo) -> Mixer + Send + 'static,
) -> Result<(DeviceInfo, DeviceThread), AudioError> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("mazzalab-audio-device".into())
        .spawn(move || {
            let stream = match open(make_mixer) {
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

fn open(
    make_mixer: impl FnOnce(&DeviceInfo) -> Mixer,
) -> Result<(DeviceInfo, cpal::Stream), AudioError> {
    let device_error = |error: &dyn std::fmt::Display| AudioError::Device(error.to_string());
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioError::Device("no output device".into()))?;
    let supported = device
        .default_output_config()
        .map_err(|e| device_error(&e))?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let info = DeviceInfo {
        name: device
            .id()
            .map_or_else(|_| "default output".to_owned(), |id| id.to_string()),
        sample_rate: config.sample_rate,
        channels: usize::from(config.channels),
    };
    let mixer = make_mixer(&info);

    let stream = match sample_format {
        SampleFormat::F32 => build::<f32>(&device, config, mixer),
        SampleFormat::F64 => build::<f64>(&device, config, mixer),
        SampleFormat::I16 => build::<i16>(&device, config, mixer),
        SampleFormat::I32 => build::<i32>(&device, config, mixer),
        SampleFormat::U16 => build::<u16>(&device, config, mixer),
        SampleFormat::U8 => build::<u8>(&device, config, mixer),
        other => {
            return Err(AudioError::Device(format!(
                "unsupported sample format {other}"
            )));
        }
    }?;
    stream.play().map_err(|e| device_error(&e))?;
    Ok((info, stream))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut mixer: Mixer,
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
                    mixer.process(buffer);
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

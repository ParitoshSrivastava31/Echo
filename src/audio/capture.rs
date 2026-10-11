use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SizedSample, Stream};
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::resampler::LinearResampler;

/// An open microphone stream. Capture starts on creation and stops when dropped,
/// so the Windows microphone indicator is only lit while you are dictating.
pub struct AudioCapture {
    _stream: Stream,
    failed: Arc<AtomicBool>,
    pub device_name: String,
}

impl AudioCapture {
    pub fn list_input_devices() -> Vec<String> {
        cpal::default_host()
            .input_devices()
            .map(|d| d.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    /// Opens `device_name` (or the default microphone) and streams 16 kHz mono chunks to `tx`.
    /// Falls back to the default device if the named one is gone.
    pub fn start(device_name: Option<&str>, tx: Sender<Vec<f32>>) -> Result<Self, String> {
        let host = cpal::default_host();
        let named = device_name.and_then(|name| {
            host.input_devices().ok()?.find(|d| d.name().map(|n| n == name).unwrap_or(false))
        });
        let device = match named {
            Some(d) => d,
            None => host.default_input_device().ok_or("No microphone found")?,
        };
        let name = device.name().unwrap_or_else(|_| "Microphone".into());
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let failed = Arc::new(AtomicBool::new(false));

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, tx, failed.clone(), |s| s),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, tx, failed.clone(), |s| s as f32 / 32768.0),
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &config, tx, failed.clone(), |s| (s as f32 - 32768.0) / 32768.0)
            }
            cpal::SampleFormat::I32 => {
                build::<i32>(&device, &config, tx, failed.clone(), |s| s as f32 / 2_147_483_648.0)
            }
            f => return Err(format!("Unsupported microphone sample format {f:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, failed, device_name: name })
    }

    /// True if the device reported an error (e.g. it was unplugged).
    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

fn build<T: SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    tx: Sender<Vec<f32>>,
    failed: Arc<AtomicBool>,
    to_f32: fn(T) -> f32,
) -> Result<Stream, String> {
    let channels = config.channels();
    let mut resampler = LinearResampler::new(config.sample_rate().0, 16_000);
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_input_stream(
            &config.config(),
            move |data: &[T], _| {
                scratch.clear();
                scratch.extend(data.iter().map(|&s| to_f32(s)));
                let out = resampler.process_interleaved(&scratch, channels);
                if !out.is_empty() {
                    // Bounded channel: if the worker ever falls behind we drop rather than block the audio thread.
                    let _ = tx.try_send(out);
                }
            },
            move |_err| failed.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|e| e.to_string())
}

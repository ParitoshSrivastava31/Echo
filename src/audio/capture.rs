use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::resampler::LinearResampler;

#[allow(dead_code)]
pub struct AudioCapture {
    _stream: Stream,
    is_capturing: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn list_input_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| d.name().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn new(
        device_name: Option<&str>,
        output_tx: Sender<Vec<f32>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) => {
                host.input_devices()?
                    .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                    .ok_or_else(|| format!("Microphone '{}' not found", name))?
            }
            None => host
                .default_input_device()
                .ok_or("No default input microphone found")?,
        };

        println!("Selected microphone: {}", device.name().unwrap_or_default());
        let default_config = device.default_input_config()?;
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();

        println!(
            "Microphone format: sample_rate={}, channels={}",
            sample_rate, channels
        );

        let is_capturing = Arc::new(AtomicBool::new(false));
        let capturing_flag = is_capturing.clone();

        let mut resampler = LinearResampler::new(sample_rate, 16000);

        let err_fn = |err| eprintln!("Audio stream error: {:?}", err);

        let stream = match default_config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[f32], _| {
                        if capturing_flag.load(Ordering::Relaxed) {
                            let mono_16k = resampler.process_interleaved(data, channels);
                            if !mono_16k.is_empty() {
                                let _ = output_tx.try_send(mono_16k);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[i16], _| {
                        if capturing_flag.load(Ordering::Relaxed) {
                            let f32_data: Vec<f32> = data
                                .iter()
                                .map(|&s| s as f32 / 32768.0)
                                .collect();
                            let mono_16k = resampler.process_interleaved(&f32_data, channels);
                            if !mono_16k.is_empty() {
                                let _ = output_tx.try_send(mono_16k);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                device.build_input_stream(
                    &default_config.into(),
                    move |data: &[u16], _| {
                        if capturing_flag.load(Ordering::Relaxed) {
                            let f32_data: Vec<f32> = data
                                .iter()
                                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                .collect();
                            let mono_16k = resampler.process_interleaved(&f32_data, channels);
                            if !mono_16k.is_empty() {
                                let _ = output_tx.try_send(mono_16k);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            _ => return Err("Unsupported audio sample format".into()),
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            is_capturing,
        })
    }

    pub fn set_recording(&self, active: bool) {
        self.is_capturing.store(active, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }
}

/// Streaming resampler from any device rate/channel count to 16 kHz mono.
/// Uses a 2nd-order low-pass before linear interpolation so 44.1/48 kHz input
/// doesn't alias into the speech band, and carries state across buffers so
/// no samples are dropped at chunk boundaries.
pub struct LinearResampler {
    pub from_rate: u32,
    pub to_rate: u32,
    step: f64,
    pos: f64,     // position of next output sample relative to `prev`
    prev: f32,    // last input sample of the previous chunk
    has_prev: bool,
    lp: Biquad,
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    enabled: bool,
}

impl Biquad {
    fn lowpass(rate: f32, cutoff: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * cutoff / rate;
        let alpha = w.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
            enabled: true,
        }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

impl LinearResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        let mut lp = Biquad::lowpass(from_rate as f32, to_rate as f32 * 0.45);
        lp.enabled = from_rate > to_rate;
        Self {
            from_rate,
            to_rate,
            step: from_rate as f64 / to_rate as f64,
            pos: 0.0,
            prev: 0.0,
            has_prev: false,
            lp,
        }
    }

    /// Resample an interleaved buffer; returns 16 kHz mono samples.
    pub fn process_interleaved(&mut self, input: &[f32], channels: u16) -> Vec<f32> {
        let ch = channels.max(1) as usize;
        let frames = input.len() / ch;
        let mut out = Vec::with_capacity((frames as f64 / self.step) as usize + 2);
        for f in 0..frames {
            let s = if ch == 1 {
                input[f]
            } else {
                input[f * ch..f * ch + ch].iter().sum::<f32>() / ch as f32
            };
            let cur = self.lp.run(s);
            if !self.has_prev {
                self.prev = cur;
                self.has_prev = true;
                continue;
            }
            // Emit every output sample whose position falls between prev (0.0) and cur (1.0).
            while self.pos < 1.0 {
                let t = self.pos as f32;
                out.push(self.prev + (cur - self.prev) * t);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.prev = cur;
        }
        out
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resampling_48k_to_16k() {
        let mut r = LinearResampler::new(48000, 16000);
        let out = r.process_interleaved(&vec![1.0f32; 4800], 1);
        assert!((out.len() as i32 - 1600).abs() <= 2);
    }

    #[test]
    fn test_no_drift_across_chunks() {
        let mut r = LinearResampler::new(44100, 16000);
        let mut total = 0;
        for _ in 0..100 {
            total += r.process_interleaved(&vec![0.1f32; 441], 1).len();
        }
        // 1 second of audio must yield 16000 samples, independent of chunking.
        assert!((total as i32 - 16000).abs() <= 2, "got {total}");
    }

    #[test]
    fn test_stereo_downmix() {
        let mut r = LinearResampler::new(16000, 16000);
        let out = r.process_interleaved(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0], 2);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}

//! Tiny pure software SDF rasterizer for the overlay pill, system tray, and icons.
//! Shapes are rendered with signed-distance functions, providing sub-pixel anti-aliasing
//! and optical scaling across all Windows monitor DPIs.
//! Inspired by the design philosophies of Dieter Rams (Braun) and Jony Ive (Apple).

#[derive(Clone, Copy, Debug)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn hex(rgb: u32, a: f32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((rgb >> 8) & 0xff) as f32 / 255.0,
            b: (rgb & 0xff) as f32 / 255.0,
            a,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn lerp(self, o: Rgba, t: f32) -> Self {
        Self {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }
}

pub const TITANIUM_WHITE: Rgba = Rgba::hex(0xF8FAFC, 1.0);
pub const TITANIUM_SILVER: Rgba = Rgba::hex(0xE2E8F0, 0.85);
pub const TITANIUM_SLATE: Rgba = Rgba::hex(0x1E2530, 0.95);
pub const TITANIUM_GRAPHITE: Rgba = Rgba::hex(0x334155, 0.90);

pub const AQUA_ACCENT: Rgba = Rgba::hex(0x00C7B1, 1.0);
pub const AQUA_ACCENT_LIVE: Rgba = Rgba::hex(0x00E5CC, 1.0);
pub const BRAUN_CORAL: Rgba = Rgba::hex(0x00C7B1, 1.0); // Unified with subtle luxury aqua
pub const BRAUN_CORAL_LIVE: Rgba = Rgba::hex(0x00E5CC, 1.0);
pub const EMERALD_ACCENT: Rgba = Rgba::hex(0x00C48C, 1.0);
pub const AMBER_ACCENT: Rgba = Rgba::hex(0xFFB020, 1.0);

pub const WHITE_CERAMIC_TOP: Rgba = Rgba::hex(0xFFFFFF, 0.96);
pub const WHITE_CERAMIC_BOTTOM: Rgba = Rgba::hex(0xF4F6F8, 0.94);
pub const BRAND_TOP: Rgba = Rgba::hex(0x262832, 1.0);
pub const BRAND_BOTTOM: Rgba = Rgba::hex(0x111217, 1.0);
pub const LIVE_TOP: Rgba = Rgba::hex(0x1E2B2A, 1.0);
pub const LIVE_BOTTOM: Rgba = Rgba::hex(0x0E1716, 1.0);

/// Premultiplied BGRA canvas (the format `UpdateLayeredWindow` expects).
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>, // premultiplied b, g, r, a
}

// Signed distance functions:
pub fn sd_round_rect(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = px.abs() - (hw - r);
    let qy = py.abs() - (hh - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

pub fn sd_squircle(px: f32, py: f32, hw: f32, hh: f32) -> f32 {
    let qx = (px.abs() / hw).powf(4.2);
    let qy = (py.abs() / hh).powf(4.2);
    ((qx + qy).powf(1.0 / 4.2) - 1.0) * hw.min(hh)
}

pub fn sd_circle(px: f32, py: f32, r: f32) -> f32 {
    px.hypot(py) - r
}

pub fn sd_capsule(px: f32, py: f32, r: f32, half_h: f32) -> f32 {
    let py_c = py.clamp(-half_h, half_h);
    px.hypot(py - py_c) - r
}

pub fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, th: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let len_sq = bax * bax + bay * bay;
    let h = if len_sq > 0.0 {
        ((pax * bax + pay * bay) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let dx = pax - bax * h;
    let dy = pay - bay * h;
    dx.hypot(dy) - th * 0.5
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }

    pub fn clear(&mut self) {
        self.px.iter_mut().for_each(|p| *p = [0.0; 4]);
    }

    #[inline]
    pub fn blend(&mut self, i: usize, c: Rgba, cov: f32) {
        let a = c.a * cov;
        if a <= 0.0 {
            return;
        }
        let d = &mut self.px[i];
        let inv = 1.0 - a;
        d[0] = c.b * a + d[0] * inv;
        d[1] = c.g * a + d[1] * inv;
        d[2] = c.r * a + d[2] * inv;
        d[3] = a + d[3] * inv;
    }

    fn bounds(&self, cx: f32, cy: f32, hw: f32, hh: f32, pad: f32) -> (usize, usize, usize, usize) {
        let x0 = (cx - hw - pad).floor().max(0.0) as usize;
        let y0 = (cy - hh - pad).floor().max(0.0) as usize;
        let x1 = ((cx + hw + pad).ceil() as usize).min(self.w);
        let y1 = ((cy + hh + pad).ceil() as usize).min(self.h);
        (x0, y0, x1, y1)
    }

    /// Rounded rectangle centred at (cx, cy) with vertical gradient and optional border.
    pub fn round_rect(
        &mut self,
        cx: f32,
        cy: f32,
        hw: f32,
        hh: f32,
        r: f32,
        top: Rgba,
        bottom: Rgba,
        border: Option<(Rgba, f32)>,
    ) {
        if hw <= 0.0 || hh <= 0.0 {
            return;
        }
        let r = r.min(hw).min(hh);
        let (x0, y0, x1, y1) = self.bounds(cx, cy, hw, hh, 1.0);
        for y in y0..y1 {
            let py = y as f32 + 0.5 - cy;
            let t = ((py + hh) / (2.0 * hh)).clamp(0.0, 1.0);
            let fill = top.lerp(bottom, t);
            for x in x0..x1 {
                let px = x as f32 + 0.5 - cx;
                let d = sd_round_rect(px, py, hw, hh, r);
                let outer = (0.5 - d).clamp(0.0, 1.0);
                if outer <= 0.0 {
                    continue;
                }
                let i = y * self.w + x;
                match border {
                    Some((bc, bw)) => {
                        let inner = (0.5 - (d + bw)).clamp(0.0, 1.0);
                        self.blend(i, fill, inner);
                        self.blend(i, bc, outer - inner);
                    }
                    None => self.blend(i, fill, outer),
                }
            }
        }
    }

    /// Soft drop shadow for a rounded rectangle.
    pub fn shadow(&mut self, cx: f32, cy: f32, hw: f32, hh: f32, r: f32, blur: f32, alpha: f32) {
        if hw <= 0.0 || hh <= 0.0 || alpha <= 0.0 {
            return;
        }
        let r = r.min(hw).min(hh);
        let (x0, y0, x1, y1) = self.bounds(cx, cy, hw, hh, blur + 1.0);
        let c = Rgba::hex(0x000000, alpha);
        for y in y0..y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - cx;
                let d = sd_round_rect(px, py, hw, hh, r);
                let t = (1.0 - (d / blur)).clamp(0.0, 1.0);
                let cov = t * t * (3.0 - 2.0 * t);
                self.blend(y * self.w + x, c, cov);
            }
        }
    }

    /// Draws a rounded vertical capsule needle.
    pub fn capsule(&mut self, cx: f32, cy: f32, r: f32, half_h: f32, fill: Rgba) {
        let (x0, y0, x1, y1) = self.bounds(cx, cy, r, half_h + r, 1.0);
        for y in y0..y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - cx;
                let d = sd_capsule(px, py, r, half_h);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(y * self.w + x, fill, cov);
                }
            }
        }
    }

    /// Draws a circle with optional glowing radial halo.
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: Rgba, glow: Option<(Rgba, f32)>) {
        let pad = glow.map(|(_, g)| g).unwrap_or(0.0) + 1.0;
        let (x0, y0, x1, y1) = self.bounds(cx, cy, r, r, pad);
        for y in y0..y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - cx;
                let d = sd_circle(px, py, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                let i = y * self.w + x;

                if let Some((glow_col, glow_rad)) = glow {
                    if d > 0.0 && d < glow_rad {
                        let t = (1.0 - (d / glow_rad)).clamp(0.0, 1.0);
                        let glow_cov = t * t;
                        self.blend(i, glow_col, glow_cov * (1.0 - cov));
                    }
                }
                if cov > 0.0 {
                    self.blend(i, fill, cov);
                }
            }
        }
    }

    /// Draws a continuous line segment with rounded endcaps.
    pub fn segment(&mut self, ax: f32, ay: f32, bx: f32, by: f32, th: f32, col: Rgba) {
        let min_x = ax.min(bx) - th - 1.0;
        let max_x = ax.max(bx) + th + 1.0;
        let min_y = ay.min(by) - th - 1.0;
        let max_y = ay.max(by) + th + 1.0;
        let x0 = min_x.floor().max(0.0) as usize;
        let y0 = min_y.floor().max(0.0) as usize;
        let x1 = (max_x.ceil() as usize).min(self.w);
        let y1 = (max_y.ceil() as usize).min(self.h);

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let d = sd_segment(px, py, ax, ay, bx, by, th);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(y * self.w + x, col, cov);
                }
            }
        }
    }

    /// Premultiplied BGRA bytes for layered windows.
    pub fn write_premultiplied(&self, out: &mut [u8]) {
        for (i, p) in self.px.iter().enumerate() {
            let o = &mut out[i * 4..i * 4 + 4];
            for k in 0..4 {
                o[k] = (p[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }

    /// Straight-alpha BGRA bytes (icons want non-premultiplied colour).
    pub fn to_straight_bgra(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.w * self.h * 4];
        for (i, p) in self.px.iter().enumerate() {
            let a = p[3].clamp(0.0, 1.0);
            let o = &mut out[i * 4..i * 4 + 4];
            if a > 0.0 {
                for k in 0..3 {
                    o[k] = ((p[k] / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
            o[3] = (a * 255.0 + 0.5) as u8;
        }
        out
    }
}

/// Draws the redesigned Echo logo inspired by Apple (superellipse squircle)
/// and Dieter Rams / Braun (harmonic acoustic waveform & signature coral indicator).
pub fn draw_logo(c: &mut Canvas, live: bool) {
    let s = c.w as f32;
    let cx = s * 0.5;
    let cy = s * 0.5;

    // Optical sizing parameters
    let (hw, hh) = if s >= 48.0 {
        (s * 0.425, s * 0.425)
    } else if s >= 32.0 {
        (s * 0.445, s * 0.445)
    } else {
        (s * 0.465, s * 0.465)
    };

    // Render pixel by pixel for pristine material, lighting, and shadow depth
    for y in 0..c.h {
        let py = y as f32 + 0.5 - cy;
        for x in 0..c.w {
            let px = x as f32 + 0.5 - cx;

            // 1. Dual-layer soft drop shadow (for s >= 32)
            let sh_alpha = if s >= 32.0 {
                let d_sh1 = sd_squircle(px, py - s * 0.016, hw + 1.0, hh + 1.0);
                let sh1 = (1.0 - (d_sh1 / (s * 0.05))).clamp(0.0, 1.0);
                let d_sh2 = sd_squircle(px, py - s * 0.008, hw + 0.5, hh + 0.5);
                let sh2 = (1.0 - (d_sh2 / (s * 0.02))).clamp(0.0, 1.0);
                ((sh1.powf(2.2) * 0.32 + sh2.powf(1.8) * 0.26) * 0.85).min(0.55)
            } else {
                0.0
            };

            // 2. Base squircle coverage
            let d_sq = sd_squircle(px, py, hw, hh);
            let cov_sq = (0.5 - d_sq).clamp(0.0, 1.0);

            if cov_sq <= 0.0 {
                if sh_alpha > 0.0 {
                    c.blend(y * c.w + x, Rgba::hex(0x000000, sh_alpha), 1.0);
                }
                continue;
            }

            // 3. Dieter Rams matte graphite / anodized space basalt material
            let t_y = ((py + hh) / (2.0 * hh)).clamp(0.0, 1.0);
            let dist_light = ((px * 0.8).hypot(py + hh * 0.45) / (hw * 1.5)).clamp(0.0, 2.0);
            let spec_light = (1.0 - dist_light * 0.5).max(0.0);

            let (mut r_bg, mut g_bg, mut b_bg) = if live {
                // Subtle warm ember resonance when actively recording
                (
                    38.0 + 12.0 * spec_light - 10.0 * t_y,
                    28.0 + 8.0 * spec_light - 10.0 * t_y,
                    32.0 + 8.0 * spec_light - 10.0 * t_y,
                )
            } else {
                (
                    24.0 + 12.0 * spec_light - 12.0 * t_y,
                    26.0 + 12.0 * spec_light - 12.0 * t_y,
                    34.0 + 15.0 * spec_light - 15.0 * t_y,
                )
            };

            // 4. Precision chamfer rim highlight along upper crest
            let t_rim = (-py / hh).max(0.0);
            let rim_bw = if s >= 64.0 { 1.5 } else { 1.0 };
            let rim_cov = (0.5 - d_sq).clamp(0.0, 1.0) - (0.5 - (d_sq + rim_bw)).clamp(0.0, 1.0);
            if rim_cov > 0.0 {
                let (rim_r, rim_g, rim_b, rim_op) = if live {
                    (255.0, 110.0, 70.0, 0.55)
                } else {
                    (255.0, 255.0, 255.0, 0.08 + 0.35 * t_rim.powf(1.8))
                };
                r_bg = r_bg * (1.0 - rim_cov) + rim_r * rim_cov * rim_op + r_bg * rim_cov * (1.0 - rim_op);
                g_bg = g_bg * (1.0 - rim_cov) + rim_g * rim_cov * rim_op + g_bg * rim_cov * (1.0 - rim_op);
                b_bg = b_bg * (1.0 - rim_cov) + rim_b * rim_cov * rim_op + b_bg * rim_cov * (1.0 - rim_op);
            }

            // Blend squircle base pixel
            let base_color = Rgba {
                r: (r_bg / 255.0).clamp(0.0, 1.0),
                g: (g_bg / 255.0).clamp(0.0, 1.0),
                b: (b_bg / 255.0).clamp(0.0, 1.0),
                a: cov_sq,
            };
            c.blend(y * c.w + x, base_color, 1.0);
        }
    }

    // 5. Acoustic Wave: Titanium needles with Gaussian harmonic envelope
    let (heights, bar_w, bar_gap, max_h): (&[f32], f32, f32, f32) = if s >= 48.0 {
        (
            &[0.18, 0.38, 0.68, 0.94, 0.68, 0.38, 0.18],
            (s * 0.036).max(2.0),
            (s * 0.038).max(2.0),
            s * 0.28,
        )
    } else if s >= 28.0 {
        (
            &[0.22, 0.52, 0.92, 0.52, 0.22],
            (s * 0.062).max(2.0),
            (s * 0.058).max(2.0),
            s * 0.32,
        )
    } else {
        // 16px to 24px: 3 bold high-contrast needles for maximum taskbar clarity
        (
            &[0.35, 0.88, 0.35],
            (s * 0.13).max(2.0),
            (s * 0.11).max(1.5),
            s * 0.38,
        )
    };

    let n = heights.len() as f32;
    let total_w = n * bar_w + (n - 1.0) * bar_gap;
    let start_x = cx - total_w / 2.0 + bar_w / 2.0;

    let (bar_top, bar_bottom) = if live {
        (Rgba::hex(0xFF8A65, 0.98), Rgba::hex(0xFF5722, 0.95))
    } else {
        (Rgba::hex(0xFFFFFF, 0.98), Rgba::hex(0xE2E8F0, 0.92))
    };

    for (i, &h_mult) in heights.iter().enumerate() {
        let bx = start_x + (i as f32) * (bar_w + bar_gap);
        let bh = (max_h * h_mult).max(bar_w);
        c.round_rect(
            bx,
            cy,
            bar_w * 0.5,
            bh * 0.5,
            bar_w * 0.5,
            bar_top,
            bar_bottom,
            None,
        );
    }

    // 6. Dieter Rams signature coral acoustic status jewel in upper right
    let dot_offset = if s >= 48.0 {
        hw * 0.66
    } else if s >= 28.0 {
        hw * 0.64
    } else {
        hw * 0.62
    };
    let dot_x = cx + dot_offset;
    let dot_y = cy - dot_offset;
    let dot_r = (s * 0.030).max(1.3);

    let coral_color = if live { BRAUN_CORAL_LIVE } else { BRAUN_CORAL };
    let glow_radius = if s >= 48.0 { s * 0.032 } else { s * 0.020 };
    c.circle(
        dot_x,
        dot_y,
        dot_r,
        coral_color,
        Some((coral_color.with_alpha(0.40), glow_radius)),
    );
}

/// Builds a multi-resolution .ico file containing the redesigned icon.
pub fn build_ico() -> Vec<u8> {
    let sizes = [16usize, 20, 24, 32, 40, 48, 64, 128, 256];
    let mut images = Vec::new();
    for &s in &sizes {
        let mut c = Canvas::new(s, s);
        draw_logo(&mut c, false);
        let bgra = c.to_straight_bgra();
        let mut img = Vec::new();
        // BITMAPINFOHEADER (height doubled for the AND mask)
        img.extend_from_slice(&40u32.to_le_bytes());
        img.extend_from_slice(&(s as i32).to_le_bytes());
        img.extend_from_slice(&((s * 2) as i32).to_le_bytes());
        img.extend_from_slice(&1u16.to_le_bytes());
        img.extend_from_slice(&32u16.to_le_bytes());
        img.extend_from_slice(&[0u8; 24]);
        for y in (0..s).rev() {
            img.extend_from_slice(&bgra[y * s * 4..(y + 1) * s * 4]);
        }
        let mask_row = s.div_ceil(32) * 4;
        img.extend(std::iter::repeat_n(0u8, mask_row * s));
        images.push((s, img));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (s, img) in &images {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += img.len();
    }
    for (_, img) in images {
        out.extend_from_slice(&img);
    }
    out
}

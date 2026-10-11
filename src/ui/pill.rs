use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromPoint, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HMONITOR, MONITORINFO,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostQuitMessage, PostThreadMessageW, RegisterClassW, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, UpdateLayeredWindow, MSG, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WM_DESTROY, WM_QUIT, WM_TIMER, WM_USER, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

use super::canvas::{
    Canvas, Rgba, AMBER_ACCENT, AQUA_ACCENT, EMERALD_ACCENT, TITANIUM_GRAPHITE, TITANIUM_SLATE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationState {
    Idle,
    Listening,
    Processing,
    Done,
    Error,
}

const WM_PILL_STATE: u32 = WM_USER + 101;
const WM_PILL_LEVEL: u32 = WM_USER + 102;

struct PillInner {
    state: DictationState,
    mode_text: String,
    audio_level: f32,
    smooth_level: f32,
    anim_time: f32,
    state_change_time: Instant,
    visible: bool,
    dpi_scale: f32,
}

static mut PILL_INNER: Option<PillInner> = None;

/// Safe controller for the floating native Win32 pill overlay.
#[derive(Clone)]
pub struct PillOverlay {
    hwnd: HWND,
    thread_id: u32,
    is_running: Arc<AtomicBool>,
}

unsafe impl Send for PillOverlay {}
unsafe impl Sync for PillOverlay {}

impl PillOverlay {
    pub fn spawn() -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let is_running = Arc::new(AtomicBool::new(true));
        let running_clone = is_running.clone();

        std::thread::Builder::new()
            .name("echo-pill-ui".to_string())
            .spawn(move || {
                unsafe {
                    let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
                    let class_name = wide_str("EchoPillOverlayClass");

                    let wnd_class = WNDCLASSW {
                        style: 0,
                        lpfnWndProc: Some(pill_wnd_proc),
                        cbClsExtra: 0,
                        cbWndExtra: 0,
                        hInstance: windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
                        hIcon: std::ptr::null_mut(),
                        hCursor: std::ptr::null_mut(),
                        hbrBackground: std::ptr::null_mut(),
                        lpszMenuName: std::ptr::null(),
                        lpszClassName: class_name.as_ptr(),
                    };

                    RegisterClassW(&wnd_class);

                    // WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT
                    // Transparent ensures mouse clicks pass right through to whatever the user was doing!
                    let ex_style = WS_EX_LAYERED
                        | WS_EX_TOPMOST
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOOLWINDOW
                        | WS_EX_TRANSPARENT;

                    let hwnd = CreateWindowExW(
                        ex_style,
                        class_name.as_ptr(),
                        wide_str("Echo Pill").as_ptr(),
                        WS_POPUP,
                        0,
                        0,
                        220,
                        50,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        wnd_class.hInstance,
                        std::ptr::null(),
                    );

                    if hwnd.is_null() {
                        let _ = tx.send(Err("Failed to create pill overlay window".to_string()));
                        return;
                    }

                    let dpi = GetDpiForWindow(hwnd);
                    let scale = (dpi as f32 / 96.0).max(1.0);

                    PILL_INNER = Some(PillInner {
                        state: DictationState::Idle,
                        mode_text: "HOLD".to_string(),
                        audio_level: 0.0,
                        smooth_level: 0.0,
                        anim_time: 0.0,
                        state_change_time: Instant::now(),
                        visible: false,
                        dpi_scale: scale,
                    });

                    // Start 60 FPS animation timer (16ms)
                    SetTimer(hwnd, 1, 16, None);

                    let _ = tx.send(Ok((hwnd as isize, tid)));

                    let mut msg: MSG = std::mem::zeroed();
                    while running_clone.load(Ordering::Relaxed) {
                        let res = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                        if res <= 0 || msg.message == WM_QUIT {
                            break;
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    if !hwnd.is_null() {
                        DestroyWindow(hwnd);
                    }
                    PILL_INNER = None;
                }
            })?;

        let (hwnd_isize, thread_id) = rx.recv()??;
        let hwnd = hwnd_isize as HWND;
        Ok(Self {
            hwnd,
            thread_id,
            is_running,
        })
    }

    pub fn set_state(&self, state: DictationState, mode: &str) {
        if self.hwnd.is_null() {
            return;
        }
        let mode_code = match mode {
            "HOLD" => 1,
            "TAP" => 2,
            _ => 0,
        };
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                self.hwnd,
                WM_PILL_STATE,
                state as usize,
                mode_code as isize,
            );
        }
    }

    pub fn set_audio_level(&self, rms: f32) {
        if self.hwnd.is_null() {
            return;
        }
        // Pack rms into u32 (0.0 .. 1.0 mapped to 0 .. 1000)
        let val = ((rms * 1000.0).clamp(0.0, 1000.0)) as usize;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                self.hwnd,
                WM_PILL_LEVEL,
                val,
                0,
            );
        }
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
        if self.thread_id != 0 {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
        }
    }
}

impl Drop for PillOverlay {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe extern "system" fn pill_wnd_proc(
    hwnd: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match msg {
        WM_PILL_STATE => {
            let state = match w_param {
                1 => DictationState::Listening,
                2 => DictationState::Processing,
                3 => DictationState::Done,
                4 => DictationState::Error,
                _ => DictationState::Idle,
            };
            let mode = match l_param {
                1 => "HOLD",
                2 => "HANDS-FREE",
                _ => "",
            };

            if let Some(ref mut inner) = PILL_INNER {
                inner.state = state;
                inner.mode_text = mode.to_string();
                inner.state_change_time = Instant::now();

                match state {
                    DictationState::Idle => {
                        inner.visible = false;
                        ShowWindow(hwnd, SW_HIDE);
                    }
                    DictationState::Listening | DictationState::Processing | DictationState::Done | DictationState::Error => {
                        if !inner.visible {
                            inner.visible = true;
                            reposition_pill(hwnd, inner.dpi_scale);
                            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        }
                    }
                }
            }
            0
        }
        WM_PILL_LEVEL => {
            let val = (w_param as f32) / 1000.0;
            if let Some(ref mut inner) = PILL_INNER {
                inner.audio_level = val;
            }
            0
        }
        WM_TIMER => {
            if let Some(ref mut inner) = PILL_INNER {
                if inner.visible {
                    inner.anim_time += 0.024;
                    // Smooth volume dampening
                    inner.smooth_level += (inner.audio_level - inner.smooth_level) * 0.35;

                    // Auto-hide if in Done state after 650ms
                    if inner.state == DictationState::Done
                        && inner.state_change_time.elapsed().as_millis() > 650
                    {
                        inner.visible = false;
                        inner.state = DictationState::Idle;
                        ShowWindow(hwnd, SW_HIDE);
                        return 0;
                    }

                    // Auto-hide if in Error state after 2500ms
                    if inner.state == DictationState::Error
                        && inner.state_change_time.elapsed().as_millis() > 2500
                    {
                        inner.visible = false;
                        inner.state = DictationState::Idle;
                        ShowWindow(hwnd, SW_HIDE);
                        return 0;
                    }

                    render_pill(hwnd, inner);
                }
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

/// Positions the pill at the bottom center of the active monitor, 56px above taskbar
unsafe fn reposition_pill(hwnd: HWND, scale: f32) {
    let mut pt: POINT = std::mem::zeroed();
    windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);

    let hmon: HMONITOR = MonitorFromPoint(pt, 1 /* MONITOR_DEFAULTTOPRIMARY */);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

    if GetMonitorInfoW(hmon, &mut mi) != 0 {
        let work = mi.rcWork;
        let mon_width = work.right - work.left;
        let mon_bottom = work.bottom;

        let pill_w = (120.0 * scale) as i32;
        let pill_h = (36.0 * scale) as i32;

        let x = work.left + (mon_width - pill_w) / 2;
        let y = mon_bottom - pill_h - (48.0 * scale) as i32;

        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            pill_w,
            pill_h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Renders the liquid glass pill with audio visualizer waveform and crisp typography
unsafe fn render_pill(hwnd: HWND, inner: &PillInner) {
    let scale = inner.dpi_scale;
    let base_w = 120.0 * scale;
    let base_h = 36.0 * scale;

    let w = base_w.round() as usize;
    let h = base_h.round() as usize;

    let mut canvas = Canvas::new(w, h);

    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let hw = (w as f32 - 8.0 * scale) / 2.0;
    let hh = (h as f32 - 8.0 * scale) / 2.0;
    let r = hh; // Full round pill capsule

    // 1. Dual-layer soft ambient drop shadow (subtle, delicate studio shadow)
    canvas.shadow(cx, cy + 2.0 * scale, hw, hh, r, 7.0 * scale, 0.14);
    canvas.shadow(cx, cy + 0.8 * scale, hw, hh, r, 2.5 * scale, 0.10);

    // 2. Pristine Jony Ive / Dieter Rams White Ceramic & Frosted Acrylic Body
    let top_color = Rgba::hex(0xFFFFFF, 0.96);
    let bottom_color = Rgba::hex(0xF4F6F8, 0.94);

    // Subtle dual-tone hairline rim border
    let (border_color, border_w) = match inner.state {
        DictationState::Listening => (AQUA_ACCENT.with_alpha(0.40), 1.0 * scale),
        DictationState::Processing => (Rgba::hex(0x00C7B1, 0.30), 1.0 * scale),
        DictationState::Done => (EMERALD_ACCENT.with_alpha(0.45), 1.0 * scale),
        DictationState::Error => (AMBER_ACCENT.with_alpha(0.45), 1.0 * scale),
        DictationState::Idle => (Rgba::hex(0x000000, 0.08), 1.0 * scale),
    };

    canvas.round_rect(
        cx,
        cy,
        hw,
        hh,
        r,
        top_color,
        bottom_color,
        Some((border_color, border_w)),
    );

    // 3. Specular reflection sheen highlight along the upper crest
    let sheen_top = Rgba::hex(0xFFFFFF, 0.40);
    let sheen_bottom = Rgba::hex(0xFFFFFF, 0.0);
    canvas.round_rect(
        cx,
        cy - hh * 0.45,
        hw - 4.0 * scale,
        hh * 0.35,
        r * 0.35,
        sheen_top,
        sheen_bottom,
        None,
    );

    // 4. Content Elements depending on state
    match inner.state {
        DictationState::Listening => {
            // Dieter Rams / Jony Ive luxury aqua status jewel on the left
            let dot_x = cx - hw + 14.0 * scale;
            let dot_y = cy;
            let dot_r = 3.2 * scale;
            let pulse = ((inner.anim_time * 4.5).sin() * 0.15 + 0.85).clamp(0.0, 1.0);

            // Breathing soft cyan photon halo
            let glow_radius = 5.5 * scale * pulse;
            canvas.circle(
                dot_x,
                dot_y,
                dot_r,
                AQUA_ACCENT,
                Some((AQUA_ACCENT.with_alpha(0.40 * pulse), glow_radius)),
            );

            // Symmetrical 7 precision harmonic Gaussian acoustic needles in slate titanium
            let bars_center_x = cx + 6.0 * scale;
            let bars_count = 7;
            let bar_w = 2.0 * scale;
            let bar_gap = 2.4 * scale;
            let total_w = bars_count as f32 * bar_w + (bars_count as f32 - 1.0) * bar_gap;
            let start_x = bars_center_x - total_w / 2.0 + bar_w / 2.0;

            let gauss = [0.20f32, 0.45, 0.78, 1.00, 0.78, 0.45, 0.20];
            let max_h = hh * 1.25;
            let min_h = 3.0 * scale;
            let vol = inner.smooth_level.clamp(0.0, 1.0);

            for (i, &g_mult) in gauss.iter().enumerate() {
                let bx = start_x + (i as f32) * (bar_w + bar_gap);
                let bh = (min_h + (max_h - min_h) * vol * g_mult).max(bar_w);
                canvas.capsule(
                    bx,
                    cy,
                    bar_w * 0.5,
                    bh * 0.5,
                    TITANIUM_SLATE,
                );
            }
        }
        DictationState::Processing => {
            // Symmetrical traveling wave shimmer across 7 needles in soft graphite
            let bars_center_x = cx;
            let bars_count = 7;
            let bar_w = 2.0 * scale;
            let bar_gap = 2.4 * scale;
            let total_w = bars_count as f32 * bar_w + (bars_count as f32 - 1.0) * bar_gap;
            let start_x = bars_center_x - total_w / 2.0 + bar_w / 2.0;

            for i in 0..bars_count {
                let bx = start_x + (i as f32) * (bar_w + bar_gap);
                let phase = ((inner.anim_time * 6.0 + i as f32 * 0.9).sin() * 0.5 + 0.5).clamp(0.2, 1.0);
                let bh = (5.0 + 10.0 * phase) * scale;
                canvas.capsule(
                    bx,
                    cy,
                    bar_w * 0.5,
                    bh * 0.5,
                    TITANIUM_GRAPHITE,
                );
            }
        }
        DictationState::Done => {
            // Symmetrical vector emerald-mint checkmark with soft photon glow
            let th = 2.0 * scale;
            canvas.circle(cx, cy, 9.0 * scale, EMERALD_ACCENT.with_alpha(0.12), Some((EMERALD_ACCENT.with_alpha(0.35), 7.0 * scale)));
            canvas.segment(cx - 4.5 * scale, cy, cx - 1.0 * scale, cy + 3.8 * scale, th, EMERALD_ACCENT);
            canvas.segment(cx - 1.0 * scale, cy + 3.8 * scale, cx + 5.5 * scale, cy - 4.0 * scale, th, EMERALD_ACCENT);
        }
        DictationState::Error => {
            // Symmetrical amber status jewel in center
            canvas.circle(
                cx,
                cy,
                5.5 * scale,
                AMBER_ACCENT,
                Some((AMBER_ACCENT.with_alpha(0.40), 7.0 * scale)),
            );
        }
        DictationState::Idle => {}
    }

    // Pass premultiplied buffer to UpdateLayeredWindow
    let mut bgra = vec![0u8; w * h * 4];
    canvas.write_premultiplied(&mut bgra);

    let hdc_screen = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
    let hdc_mem = CreateCompatibleDC(hdc_screen);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // Top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: (w * h * 4) as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [std::mem::zeroed(); 1],
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(
        hdc_mem,
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        std::ptr::null_mut(),
        0,
    );

    if !hbmp.is_null() && !bits.is_null() {
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        let old_bmp = SelectObject(hdc_mem, hbmp);

        let mut pt_src = POINT { x: 0, y: 0 };
        let mut size = SIZE { cx: w as i32, cy: h as i32 };

        let mut rect: RECT = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);
        let mut pt_dst = POINT { x: rect.left, y: rect.top };

        let blend = BLENDFUNCTION {
            BlendOp: 0, // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1, // AC_SRC_ALPHA
        };

        UpdateLayeredWindow(
            hwnd,
            hdc_screen,
            &mut pt_dst,
            &mut size,
            hdc_mem,
            &mut pt_src,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(hdc_mem, old_bmp);
        DeleteObject(hbmp);
    }

    DeleteDC(hdc_mem);
    windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), hdc_screen);
}

fn wide_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

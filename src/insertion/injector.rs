use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RWIN, VK_SHIFT, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

pub struct TextInjector;

impl TextInjector {
    /// Return the currently focused window handle.
    pub fn get_focused_window() -> HWND {
        unsafe { GetForegroundWindow() }
    }

    /// Wait for modifier keys (Win, Alt, Ctrl, Shift) to be released so they don't combine with typed text.
    pub fn wait_for_modifiers_released() {
        let deadline = Instant::now() + Duration::from_millis(150);
        let modifiers = [
            VK_LWIN, VK_RWIN, VK_LMENU, VK_RMENU, VK_MENU,
            VK_LCONTROL, VK_RCONTROL, VK_CONTROL, VK_SHIFT,
        ];

        while Instant::now() < deadline {
            let any_down = modifiers.iter().any(|&vk| unsafe {
                (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0
            });
            if !any_down {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        // Send explicit KEYUP for modifier keys to guarantee clean keyboard state
        let mut keyups: Vec<INPUT> = Vec::with_capacity(modifiers.len());
        for &vk in &modifiers {
            let mut up: INPUT = unsafe { std::mem::zeroed() };
            up.r#type = INPUT_KEYBOARD;
            up.Anonymous.ki.wVk = vk;
            up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
            keyups.push(up);
        }
        unsafe {
            SendInput(
                keyups.len() as u32,
                keyups.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
        thread::sleep(Duration::from_millis(10));
    }

    /// Insert Unicode text directly at the cursor in the currently focused application.
    pub fn inject_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
        if text.is_empty() {
            return Ok(());
        }

        Self::wait_for_modifiers_released();

        let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 4);

        for ch in text.chars() {
            if ch == '\n' {
                let mut down: INPUT = unsafe { std::mem::zeroed() };
                down.r#type = INPUT_KEYBOARD;
                down.Anonymous.ki.wVk = 0x0D; // VK_RETURN
                down.Anonymous.ki.dwFlags = 0;

                let mut up: INPUT = unsafe { std::mem::zeroed() };
                up.r#type = INPUT_KEYBOARD;
                up.Anonymous.ki.wVk = 0x0D;
                up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

                inputs.push(down);
                inputs.push(up);
            } else {
                let mut utf16_buf = [0u16; 2];
                let encoded = ch.encode_utf16(&mut utf16_buf);

                for &code_unit in encoded.iter() {
                    let mut down: INPUT = unsafe { std::mem::zeroed() };
                    down.r#type = INPUT_KEYBOARD;
                    down.Anonymous.ki.wScan = code_unit;
                    down.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE;

                    let mut up: INPUT = unsafe { std::mem::zeroed() };
                    up.r#type = INPUT_KEYBOARD;
                    up.Anonymous.ki.wScan = code_unit;
                    up.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;

                    inputs.push(down);
                    inputs.push(up);
                }
            }
        }

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr() as *const INPUT,
                std::mem::size_of::<INPUT>() as i32,
            )
        };

        if sent != inputs.len() as u32 {
            // If SendInput failed or was blocked by elevated app, try clipboard fallback
            return Self::inject_via_clipboard(text);
        }

        Ok(())
    }

    /// Send `count` backspace keystrokes to erase text (used for "scratch that" and corrections).
    pub fn send_backspaces(count: usize) {
        if count == 0 {
            return;
        }

        Self::wait_for_modifiers_released();

        let mut inputs: Vec<INPUT> = Vec::with_capacity(count * 2);

        for _ in 0..count {
            let mut down: INPUT = unsafe { std::mem::zeroed() };
            down.r#type = INPUT_KEYBOARD;
            down.Anonymous.ki.wVk = VK_BACK;
            down.Anonymous.ki.dwFlags = 0;

            let mut up: INPUT = unsafe { std::mem::zeroed() };
            up.r#type = INPUT_KEYBOARD;
            up.Anonymous.ki.wVk = VK_BACK;
            up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

            inputs.push(down);
            inputs.push(up);
        }

        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr() as *const INPUT,
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }

    /// Clipboard fallback: preserves original clipboard, copies text, sends Ctrl+V, then restores.
    pub fn inject_via_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let previous_text = Self::get_clipboard_text();

            if Self::set_clipboard_text(text) {
                // Send Ctrl+V
                let mut paste_inputs: [INPUT; 4] = std::mem::zeroed();

                // Ctrl down
                paste_inputs[0].r#type = INPUT_KEYBOARD;
                paste_inputs[0].Anonymous.ki.wVk = VK_CONTROL;

                // V down
                paste_inputs[1].r#type = INPUT_KEYBOARD;
                paste_inputs[1].Anonymous.ki.wVk = VK_V;

                // V up
                paste_inputs[2].r#type = INPUT_KEYBOARD;
                paste_inputs[2].Anonymous.ki.wVk = VK_V;
                paste_inputs[2].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

                // Ctrl up
                paste_inputs[3].r#type = INPUT_KEYBOARD;
                paste_inputs[3].Anonymous.ki.wVk = VK_CONTROL;
                paste_inputs[3].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

                SendInput(4, paste_inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);

                // Give target app a moment to receive the paste before restoring clipboard
                thread::sleep(Duration::from_millis(80));

                if let Some(prev) = previous_text {
                    let _ = Self::set_clipboard_text(&prev);
                }
            }
        }

        Ok(())
    }

    unsafe fn get_clipboard_text() -> Option<String> {
        const CF_UNICODETEXT: u32 = 13;
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        let result = if !handle.is_null() {
            let ptr = GlobalLock(handle) as *const u16;
            if !ptr.is_null() {
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len);
                let s = String::from_utf16_lossy(slice);
                GlobalUnlock(handle);
                Some(s)
            } else {
                None
            }
        } else {
            None
        };
        CloseClipboard();
        result
    }

    unsafe fn set_clipboard_text(text: &str) -> bool {
        const CF_UNICODETEXT: u32 = 13;
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();

        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = utf16.len() * 2;
        let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes);

        if !h_mem.is_null() {
            let ptr = GlobalLock(h_mem) as *mut u16;
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
                GlobalUnlock(h_mem);
                let ok = SetClipboardData(CF_UNICODETEXT, h_mem);
                CloseClipboard();
                return !ok.is_null();
            }
        }
        CloseClipboard();
        false
    }
}

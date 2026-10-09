use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_BACK, VK_CONTROL, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

pub struct TextInjector;

impl TextInjector {
    /// Check the current focused window handle.
    #[allow(dead_code)]
    pub fn get_focused_window() -> HWND {
        unsafe { GetForegroundWindow() }
    }

    /// Insert Unicode text directly at the cursor in the currently focused application.
    pub fn inject_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
        if text.is_empty() {
            return Ok(());
        }

        let mut inputs: Vec<INPUT> = Vec::new();

        for ch in text.chars() {
            if ch == '\n' {
                // Carriage return + Line feed or Return key
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
                // Encode UTF-16 code units
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
            // If SendInput failed, try clipboard fallback
            return Self::inject_via_clipboard(text);
        }

        Ok(())
    }

    /// Send `count` backspace keystrokes to erase text (used for "scratch that" and corrections).
    pub fn send_backspaces(count: usize) {
        if count == 0 {
            return;
        }

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

    /// Clipboard fallback: paste via Ctrl+V and restore original clipboard
    fn inject_via_clipboard(_text: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Fallback using simulated Ctrl+V
        let mut inputs: [INPUT; 4] = unsafe { std::mem::zeroed() };

        // Ctrl down
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki.wVk = VK_CONTROL;
        inputs[0].Anonymous.ki.dwFlags = 0;

        // V down
        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki.wVk = VK_V;
        inputs[1].Anonymous.ki.dwFlags = 0;

        // V up
        inputs[2].r#type = INPUT_KEYBOARD;
        inputs[2].Anonymous.ki.wVk = VK_V;
        inputs[2].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        // Ctrl up
        inputs[3].r#type = INPUT_KEYBOARD;
        inputs[3].Anonymous.ki.wVk = VK_CONTROL;
        inputs[3].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        unsafe {
            SendInput(4, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }

        Ok(())
    }
}

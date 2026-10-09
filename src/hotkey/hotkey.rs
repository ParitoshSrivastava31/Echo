use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
};

#[allow(dead_code)]
pub struct HotkeyManager {
    is_running: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl HotkeyManager {
    /// Start listening for global hotkey on a dedicated background thread.
    /// Default: Ctrl + Alt + Space.
    pub fn start_listening(
        hotkey_tx: Sender<()>,
        modifiers: u32,
        vk_key: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let is_running = Arc::new(AtomicBool::new(true));
        let running_flag = is_running.clone();

        thread::Builder::new()
            .name("echo-hotkey-worker".to_string())
            .spawn(move || {
                unsafe {
                    let hotkey_id = 1;
                    let reg = RegisterHotKey(
                        std::ptr::null_mut(),
                        hotkey_id,
                        modifiers | MOD_NOREPEAT,
                        vk_key,
                    );

                    if reg == 0 {
                        eprintln!("Failed to register global hotkey (Error code from Win32)");
                        return;
                    }

                    println!("Global hotkey successfully registered (Ctrl+Alt+Space)!");

                    let mut msg: MSG = std::mem::zeroed();
                    while running_flag.load(Ordering::Relaxed) {
                        let res = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                        if res <= 0 {
                            break;
                        }

                        if msg.message == WM_HOTKEY && msg.wParam == hotkey_id as usize {
                            let _ = hotkey_tx.send(());
                        }

                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    UnregisterHotKey(std::ptr::null_mut(), hotkey_id);
                }
            })?;

        Ok(Self { is_running })
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

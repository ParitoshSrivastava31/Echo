use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RWIN, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Start { mode: &'static str },
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoKeyCombo {
    WinAlt,
    AltE,
    AltSpace,
    CtrlSpace,
    WinSpace,
    CtrlAlt,
}

impl TwoKeyCombo {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().replace([' ', '+'], "").as_str() {
            "winalt" => TwoKeyCombo::WinAlt,
            "alte" => TwoKeyCombo::AltE,
            "altspace" => TwoKeyCombo::AltSpace,
            "ctrlspace" => TwoKeyCombo::CtrlSpace,
            "winspace" => TwoKeyCombo::WinSpace,
            "ctrlalt" => TwoKeyCombo::CtrlAlt,
            _ => TwoKeyCombo::WinAlt,
        }
    }

    pub fn to_display_str(&self) -> &'static str {
        match self {
            TwoKeyCombo::WinAlt => "Win + Alt",
            TwoKeyCombo::AltE => "Alt + E",
            TwoKeyCombo::AltSpace => "Alt + Space",
            TwoKeyCombo::CtrlSpace => "Ctrl + Space",
            TwoKeyCombo::WinSpace => "Win + Space",
            TwoKeyCombo::CtrlAlt => "Ctrl + Alt",
        }
    }

    pub fn matches_key1(&self, vk: u32) -> bool {
        match self {
            TwoKeyCombo::WinAlt | TwoKeyCombo::WinSpace => vk == VK_LWIN as u32 || vk == VK_RWIN as u32,
            TwoKeyCombo::AltSpace | TwoKeyCombo::AltE => vk == VK_LMENU as u32 || vk == VK_RMENU as u32 || vk == VK_MENU as u32,
            TwoKeyCombo::CtrlSpace | TwoKeyCombo::CtrlAlt => vk == VK_LCONTROL as u32 || vk == VK_RCONTROL as u32 || vk == VK_CONTROL as u32,
        }
    }

    pub fn matches_key2(&self, vk: u32) -> bool {
        match self {
            TwoKeyCombo::WinAlt | TwoKeyCombo::CtrlAlt => vk == VK_LMENU as u32 || vk == VK_RMENU as u32 || vk == VK_MENU as u32,
            TwoKeyCombo::AltE => vk == 0x45 || vk == b'E' as u32 || vk == b'e' as u32,
            TwoKeyCombo::AltSpace | TwoKeyCombo::CtrlSpace | TwoKeyCombo::WinSpace => vk == VK_SPACE as u32,
        }
    }
}


struct HookState {
    combo: TwoKeyCombo,
    key1_down: bool,
    key2_down: bool,
    both_down: bool,
    press_instant: Option<Instant>,
    is_in_toggle_session: bool,
    action_tx: Option<Sender<HotkeyAction>>,
}

static mut HOOK_HANDLE: HHOOK = std::ptr::null_mut();
static mut HOOK_STATE: Option<HookState> = None;

const HOLD_THRESHOLD_MS: u128 = 320;

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let kbd_struct = *(l_param as *const KBDLLHOOKSTRUCT);
        let vk = kbd_struct.vkCode;
        let msg = w_param as u32;

        let is_injected = (kbd_struct.flags & 1) != 0;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if !is_injected {
            if let Some(ref mut state) = HOOK_STATE {
                let is_k1 = state.combo.matches_key1(vk);
                let is_k2 = state.combo.matches_key2(vk);

                // Track physical non-hotkey typing for cursor position change detection
                if !is_k1 && !is_k2 && is_down {
                    KEY_COUNTER.fetch_add(1, Ordering::Relaxed);
                }

                if is_k1 || is_k2 {
                    if is_down {
                        if is_k1 {
                            state.key1_down = true;
                        }
                        if is_k2 {
                            state.key2_down = true;
                        }

                        // Both keys down simultaneously
                        if state.key1_down && state.key2_down && !state.both_down {
                            state.both_down = true;
                            state.press_instant = Some(Instant::now());

                            if state.is_in_toggle_session {
                                // Tap while already hands-free -> stop dictation!
                                state.is_in_toggle_session = false;
                                if let Some(ref tx) = state.action_tx {
                                    let _ = tx.send(HotkeyAction::Stop);
                                }
                            } else {
                                // Start listening
                                if let Some(ref tx) = state.action_tx {
                                    let _ = tx.send(HotkeyAction::Start { mode: "HOLD" });
                                }
                            }
                        }
                    } else if is_up {
                        let was_both_down = state.both_down;

                        if is_k1 {
                            state.key1_down = false;
                        }
                        if is_k2 {
                            state.key2_down = false;
                        }

                        // Either key released after both were held
                        if was_both_down && (!state.key1_down || !state.key2_down) {
                            state.both_down = false;
                            if let Some(press_start) = state.press_instant.take() {
                                let duration = press_start.elapsed().as_millis();
                                if duration >= HOLD_THRESHOLD_MS {
                                    // PUSH-TO-TALK: Release stops and inserts
                                    state.is_in_toggle_session = false;
                                    if let Some(ref tx) = state.action_tx {
                                        let _ = tx.send(HotkeyAction::Stop);
                                    }
                                } else {
                                    // TAP-TO-TOGGLE: Short tap switches to hands-free mode
                                    state.is_in_toggle_session = true;
                                    if let Some(ref tx) = state.action_tx {
                                        let _ = tx.send(HotkeyAction::Start { mode: "TAP" });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    CallNextHookEx(HOOK_HANDLE, n_code, w_param, l_param)
}

pub struct HotkeyManager {
    is_running: Arc<AtomicBool>,
    thread_id: Arc<std::sync::atomic::AtomicU32>,
}

impl HotkeyManager {
    pub fn start_listening(
        action_tx: Sender<HotkeyAction>,
        combo: TwoKeyCombo,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let is_running = Arc::new(AtomicBool::new(true));
        let thread_id = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let is_running_clone = is_running.clone();
        let thread_id_clone = thread_id.clone();

        thread::Builder::new()
            .name("echo-hotkey-ll".to_string())
            .spawn(move || {
                unsafe {
                    let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
                    thread_id_clone.store(tid, Ordering::SeqCst);

                    HOOK_STATE = Some(HookState {
                        combo,
                        key1_down: false,
                        key2_down: false,
                        both_down: false,
                        press_instant: None,
                        is_in_toggle_session: false,
                        action_tx: Some(action_tx),
                    });

                    let hook = SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(low_level_keyboard_proc),
                        std::ptr::null_mut(),
                        0,
                    );

                    if hook.is_null() {
                        crate::log::info("Failed to install low-level keyboard hook (WH_KEYBOARD_LL)");
                        return;
                    }

                    HOOK_HANDLE = hook;
                    crate::log::info(&format!("Keyboard hook active for {}", combo.to_display_str()));

                    let mut msg: MSG = std::mem::zeroed();
                    while is_running_clone.load(Ordering::Relaxed) {
                        let res = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                        if res <= 0 || msg.message == WM_QUIT {
                            break;
                        }

                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    if !HOOK_HANDLE.is_null() {
                        UnhookWindowsHookEx(HOOK_HANDLE);
                        HOOK_HANDLE = std::ptr::null_mut();
                    }
                    HOOK_STATE = None;
                }
            })?;

        thread::sleep(Duration::from_millis(50));

        Ok(Self {
            is_running,
            thread_id,
        })
    }

    pub fn set_combo(&self, combo: TwoKeyCombo) {
        unsafe {
            if let Some(ref mut state) = HOOK_STATE {
                state.combo = combo;
                state.key1_down = false;
                state.key2_down = false;
                state.both_down = false;
                state.is_in_toggle_session = false;
            }
        }
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let tid = self.thread_id.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                PostThreadMessageW(tid, WM_QUIT, 0, 0);
            }
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.stop();
    }
}

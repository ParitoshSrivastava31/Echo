use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crossbeam_channel::Sender;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW,
    DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetMessageW, GetSystemMetrics, LoadIconW, LoadImageW, PostQuitMessage,
    PostThreadMessageW, RegisterClassW, SetForegroundWindow, TrackPopupMenuEx,
    TranslateMessage, HICON, HMENU, IMAGE_ICON, LR_DEFAULTCOLOR,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, MSG, SM_CXSMICON, SM_CYSMICON, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_RETURNCMD, WM_DESTROY, WM_LBUTTONUP, WM_QUIT,
    WM_RBUTTONUP, WM_USER, WNDCLASSW, WS_OVERLAPPED,
};


use crate::audio::AudioCapture;

use crate::config::AppConfig;

const WM_TRAY_CALLBACK: u32 = WM_USER + 201;

pub enum TrayCommand {
    ChangeHotkey(String),
    ChangeMicrophone(Option<String>),
    ToggleSuppressFillers,
    ToggleAutostart,
    OpenDictionary,
    OpenLogs,
    RunBenchmark,
    Quit,
}

use std::sync::Mutex;

pub struct TrayManager {
    _hwnd: HWND,
    thread_id: u32,
    is_running: Arc<AtomicBool>,
}

unsafe impl Send for TrayManager {}
unsafe impl Sync for TrayManager {}

static TRAY_CMD_TX: Mutex<Option<Sender<TrayCommand>>> = Mutex::new(None);
static TRAY_CONFIG: Mutex<Option<AppConfig>> = Mutex::new(None);

impl TrayManager {
    pub fn spawn(
        cmd_tx: Sender<TrayCommand>,
        config: AppConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let is_running = Arc::new(AtomicBool::new(true));
        let running_clone = is_running.clone();

        std::thread::Builder::new()
            .name("echo-tray".to_string())
            .spawn(move || {
                unsafe {
                    let tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
                    *TRAY_CMD_TX.lock().unwrap() = Some(cmd_tx);
                    *TRAY_CONFIG.lock().unwrap() = Some(config);

                    let class_name = wide_str("EchoTrayMsgWindowClass");
                    let wnd_class = WNDCLASSW {
                        style: 0,
                        lpfnWndProc: Some(tray_wnd_proc),
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

                    let hwnd = CreateWindowExW(
                        0,
                        class_name.as_ptr(),
                        wide_str("Echo Tray").as_ptr(),
                        WS_OVERLAPPED,
                        0,
                        0,
                        0,
                        0,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        wnd_class.hInstance,
                        std::ptr::null(),
                    );

                    if hwnd.is_null() {
                        let _ = tx.send(Err("Failed to create tray hidden window".to_string()));
                        return;
                    }

                    // Create 32x32 native HICON from logo
                    let hicon = create_app_hicon(false);
                    init_tray_icon(hwnd, hicon);

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

                    remove_tray_icon(hwnd);
                    if !hwnd.is_null() {
                        DestroyWindow(hwnd);
                    }
                }
            })?;

        let (hwnd_isize, thread_id) = rx.recv()??;
        let hwnd = hwnd_isize as HWND;
        Ok(Self {
            _hwnd: hwnd,
            thread_id,
            is_running,
        })
    }

    pub fn update_config(&self, config: AppConfig) {
        *TRAY_CONFIG.lock().unwrap() = Some(config);
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

impl Drop for TrayManager {
    fn drop(&mut self) {
        self.stop();
    }
}

unsafe fn create_app_hicon(live: bool) -> HICON {
    let cx = GetSystemMetrics(SM_CXSMICON);
    let cy = GetSystemMetrics(SM_CYSMICON);

    // 1. If not live status, load the embedded high-resolution PE icon (Resource ID 1 from echo.rc)
    if !live {
        let hinstance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        let hicon = LoadImageW(
            hinstance,
            1 as *const u16,
            IMAGE_ICON,
            cx,
            cy,
            LR_DEFAULTCOLOR,
        );
        if !hicon.is_null() {
            return hicon;
        }

        let hicon = LoadIconW(hinstance, 1 as *const u16);
        if !hicon.is_null() {
            return hicon;
        }
    }

    // 2. Synthesize dynamically with valid Windows RT_ICON resource format (BITMAPINFOHEADER + bottom-up BGRA + mask)
    let size = if cx > 0 { cx as usize } else { 32usize };
    let icon_res = super::canvas::build_icon_resource(size, live);
    CreateIconFromResourceEx(
        icon_res.as_ptr(),
        icon_res.len() as u32,
        1, // TRUE = icon
        0x00030000,
        size as i32,
        size as i32,
        0,
    )
}


unsafe fn init_tray_icon(hwnd: HWND, hicon: HICON) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_CALLBACK;
    nid.hIcon = hicon;

    let tip = wide_str("Echo — Offline Voice Keyboard (Win+Alt)");
    let len = tip.len().min(nid.szTip.len() - 1);
    nid.szTip[..len].copy_from_slice(&tip[..len]);

    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = l_param as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_tray_menu(hwnd);
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

unsafe fn show_tray_menu(hwnd: HWND) {
    let mut pt: POINT = std::mem::zeroed();
    GetCursorPos(&mut pt);

    SetForegroundWindow(hwnd);

    let hmenu: HMENU = CreatePopupMenu();
    let config = TRAY_CONFIG.lock().unwrap().clone().unwrap_or_default();

    // 1. App Title (Disabled Header)
    AppendMenuW(hmenu, MF_STRING | MF_GRAYED, 1001, wide_str("Echo · Offline Voice Keyboard").as_ptr());
    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

    // 2. Shortcut Submenu
    let h_sub_hotkey: HMENU = CreatePopupMenu();
    let current_combo = config.hotkey_combo.to_lowercase().replace(' ', "");

    let combos = [
        ("Win + Alt (Recommended)", "winalt", 2001),
        ("Alt + E", "alte", 2006),
        ("Alt + Space", "altspace", 2002),
        ("Ctrl + Space", "ctrlspace", 2003),
        ("Win + Space", "winspace", 2004),
        ("Ctrl + Alt", "ctrlalt", 2005),
    ];


    for (label, key, id) in combos {
        let flag = if current_combo == key { MF_CHECKED } else { MF_UNCHECKED };
        AppendMenuW(h_sub_hotkey, MF_STRING | flag, id, wide_str(label).as_ptr());
    }
    AppendMenuW(hmenu, MF_POPUP, h_sub_hotkey as usize, wide_str("Shortcut Key").as_ptr());

    // 3. Microphone Submenu
    let h_sub_mic: HMENU = CreatePopupMenu();
    let devices = AudioCapture::list_input_devices();
    let selected_mic = config.microphone_name.clone();

    // Default option
    let def_flag = if selected_mic.is_none() { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(h_sub_mic, MF_STRING | def_flag, 3000, wide_str("Default Microphone").as_ptr());

    for (i, d) in devices.iter().enumerate() {
        let flag = if selected_mic.as_deref() == Some(d.as_str()) {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        let id = 3001 + i as usize;
        AppendMenuW(h_sub_mic, MF_STRING | flag, id, wide_str(d).as_ptr());
    }
    AppendMenuW(hmenu, MF_POPUP, h_sub_mic as usize, wide_str("Microphone").as_ptr());

    // 4. Preferences
    let filler_flag = if config.suppress_fillers { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(hmenu, MF_STRING | filler_flag, 4001, wide_str("Filter filler words (um, uh)").as_ptr());

    let autostart_flag = if is_autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
    AppendMenuW(hmenu, MF_STRING | autostart_flag, 4002, wide_str("Start with Windows").as_ptr());

    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

    // 5. Actions
    AppendMenuW(hmenu, MF_STRING, 5001, wide_str("Open Personal Dictionary (JSON)...").as_ptr());
    AppendMenuW(hmenu, MF_STRING, 5002, wide_str("Open Log File...").as_ptr());
    AppendMenuW(hmenu, MF_STRING, 5003, wide_str("Run ASR Benchmark").as_ptr());

    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

    // 6. Exit
    AppendMenuW(hmenu, MF_STRING, 9999, wide_str("Exit Echo").as_ptr());

    let cmd = TrackPopupMenuEx(
        hmenu,
        TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        hwnd,
        std::ptr::null(),
    );

    DestroyMenu(hmenu);

    if cmd != 0 {
        handle_menu_cmd(cmd as usize, &devices);
    }
}

unsafe fn handle_menu_cmd(cmd: usize, devices: &[String]) {
    let guard = TRAY_CMD_TX.lock().unwrap();
    let Some(ref tx) = *guard else { return };

    match cmd {
        2001 => { let _ = tx.send(TrayCommand::ChangeHotkey("Win+Alt".into())); }
        2006 => { let _ = tx.send(TrayCommand::ChangeHotkey("Alt+E".into())); }
        2002 => { let _ = tx.send(TrayCommand::ChangeHotkey("Alt+Space".into())); }
        2003 => { let _ = tx.send(TrayCommand::ChangeHotkey("Ctrl+Space".into())); }
        2004 => { let _ = tx.send(TrayCommand::ChangeHotkey("Win+Space".into())); }
        2005 => { let _ = tx.send(TrayCommand::ChangeHotkey("Ctrl+Alt".into())); }
        3000 => { let _ = tx.send(TrayCommand::ChangeMicrophone(None)); }
        id if (3001..3001 + devices.len()).contains(&id) => {
            let mic_name = devices[id - 3001].clone();
            let _ = tx.send(TrayCommand::ChangeMicrophone(Some(mic_name)));
        }
        4001 => { let _ = tx.send(TrayCommand::ToggleSuppressFillers); }
        4002 => { let _ = tx.send(TrayCommand::ToggleAutostart); }
        5001 => { let _ = tx.send(TrayCommand::OpenDictionary); }
        5002 => { let _ = tx.send(TrayCommand::OpenLogs); }
        5003 => { let _ = tx.send(TrayCommand::RunBenchmark); }
        9999 => { let _ = tx.send(TrayCommand::Quit); }
        _ => {}
    }
}

pub fn is_autostart_enabled() -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };

    unsafe {
        let subkey = wide_str(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let val_name = wide_str("EchoVoiceKeyboard");
        let mut hkey: HKEY = std::ptr::null_mut();

        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut val_type: u32 = 0;
            let mut data_len: u32 = 0;
            let res = RegQueryValueExW(
                hkey,
                val_name.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                std::ptr::null_mut(),
                &mut data_len,
            );
            RegCloseKey(hkey);
            res == 0
        } else {
            false
        }
    }
}

pub fn set_autostart_enabled(enable: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_WRITE, REG_SZ,
    };

    unsafe {
        let subkey = wide_str(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let val_name = wide_str("EchoVoiceKeyboard");
        let mut hkey: HKEY = std::ptr::null_mut();

        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_WRITE, &mut hkey) != 0 {
            return Err("Failed to open Windows Run registry key".into());
        }

        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let path_str = format!("\"{}\"", exe.to_string_lossy());
            let path_wide = wide_str(&path_str);
            let byte_len = (path_wide.len() * 2) as u32;

            let res = RegSetValueExW(
                hkey,
                val_name.as_ptr(),
                0,
                REG_SZ,
                path_wide.as_ptr() as *const u8,
                byte_len,
            );
            RegCloseKey(hkey);
            if res != 0 {
                return Err("Failed to set registry value".into());
            }
        } else {
            RegDeleteValueW(hkey, val_name.as_ptr());
            RegCloseKey(hkey);
        }
        Ok(())
    }
}

fn wide_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_app_hicon_not_null() {
        unsafe {
            let hicon = create_app_hicon(false);
            assert!(!hicon.is_null(), "HICON must not be null");
            let hicon_live = create_app_hicon(true);
            assert!(!hicon_live.is_null(), "Live HICON must not be null");
        }
    }
}


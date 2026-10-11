//! Minimal local log file. Never receives audio or transcript text.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: Mutex<Option<File>> = Mutex::new(None);

pub fn init() {
    let dir = crate::paths::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("echo.log");
    let too_big = std::fs::metadata(&path).map(|m| m.len() > 512 * 1024).unwrap_or(false);
    let file = OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(path)
        .ok();
    *LOG.lock().unwrap() = file;
}

pub fn info(msg: &str) {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Some(f) = LOG.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{ts} {msg}");
    }
    if cfg!(debug_assertions) {
        eprintln!("[echo] {msg}");
    }
}

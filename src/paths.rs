use std::path::{Path, PathBuf};

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Looks for a bundled resource next to the exe, then in parent folders
/// (so `target/release/echo.exe` finds `./models` during development), then the CWD.
pub fn find_resource(rel: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = exe_dir().ancestors().take(4).map(|d| d.join(rel)).collect();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(rel));
    }
    candidates.into_iter().find(|p| p.exists())
}

fn env_dir(var: &str) -> PathBuf {
    let base = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(exe_dir);
    base.join("Echo")
}

/// Roaming settings: `%APPDATA%\Echo`.
pub fn config_dir() -> PathBuf {
    env_dir("APPDATA")
}

/// Local, machine-specific data such as logs: `%LOCALAPPDATA%\Echo`.
pub fn data_dir() -> PathBuf {
    env_dir("LOCALAPPDATA")
}

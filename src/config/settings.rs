use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub model_dir: String,
    pub microphone_name: Option<String>,
    pub hotkey_modifiers: u32, // e.g. MOD_CONTROL | MOD_ALT = 0x0002 | 0x0001 = 3
    pub hotkey_key: u32,       // e.g. VK_SPACE = 0x20
    pub suppress_fillers: bool,
    pub vad_threshold: f32,
    pub telemetry_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            model_dir: "models/asr4all-s".to_string(),
            microphone_name: None,
            hotkey_modifiers: 0x0002 | 0x0001, // Ctrl + Alt
            hotkey_key: 0x20,                  // Space
            suppress_fillers: true,
            vad_threshold: 0.5,
            telemetry_enabled: false,
        }
    }
}

impl AppConfig {
    pub fn default_config_path() -> PathBuf {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        Path::new(&appdata).join("Echo").join("config.json")
    }

    pub fn load_or_default() -> Self {
        let path = Self::default_config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                return config;
            }
        }
        let config = Self::default();
        let _ = config.save();
        config
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::default_config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

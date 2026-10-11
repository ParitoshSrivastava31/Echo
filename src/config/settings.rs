use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_hotkey_combo() -> String {
    "Win+Alt".to_string()
}

fn default_inference_threads() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default = "default_hotkey_combo", alias = "hotkey")]
    pub hotkey_combo: String, // "Win+Alt", "Alt+Space", "Ctrl+Space", etc.
    pub microphone_name: Option<String>,
    pub suppress_fillers: bool,
    #[serde(default = "default_inference_threads")]
    pub inference_threads: usize,
    #[serde(default)]
    pub autostart: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            hotkey_combo: "Win+Alt".to_string(),
            microphone_name: None,
            suppress_fillers: true,
            inference_threads: 2,
            autostart: false,
        }
    }
}

impl AppConfig {
    pub fn default_config_path() -> PathBuf {
        crate::paths::config_dir().join("config.json")
    }

    pub fn load_or_default() -> Self {
        let path = Self::default_config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                return config;
            }
            // If corrupt, backup original file before resetting
            let backup_path = path.with_extension("json.bak");
            let _ = fs::rename(&path, backup_path);
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

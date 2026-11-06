use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::i18n::Language;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub last_device_name: Option<String>,
    pub last_channel_left: Option<usize>,
    pub last_channel_right: Option<usize>,
    pub export_directory: Option<PathBuf>,
    pub timer_minutes: Option<u32>,
    pub silence_autostop: Option<bool>,
    pub normalization_mode: Option<String>,
    pub language: Option<Language>,
    pub last_tape_name: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_device_name: None,
            last_channel_left: Some(10),  // Default to Zoom L-12 Kanäle 11/12 (0-indexed: 10/11)
            last_channel_right: Some(11),
            export_directory: None,
            timer_minutes: Some(45),
            silence_autostop: Some(true),
            normalization_mode: Some("none".into()),
            language: Some(Language::De),
            last_tape_name: Some("Tonband 01".into()),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join("Library/Application Support/TapeDigizer/config.json");
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                return PathBuf::from(appdata).join("TapeDigizer/config.json");
            }
        }
        std::env::temp_dir().join("tapedigizer_config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(file_content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<Self>(&file_content) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_save_load() {
        let mut cfg = AppConfig::default();
        cfg.last_device_name = Some("ZOOM L-12 Driver".to_string());
        cfg.last_channel_left = Some(10);
        cfg.last_channel_right = Some(11);
        cfg.timer_minutes = Some(45);
        cfg.save();

        let loaded = AppConfig::load();
        assert_eq!(loaded.last_device_name.as_deref(), Some("ZOOM L-12 Driver"));
        assert_eq!(loaded.last_channel_left, Some(10));
        assert_eq!(loaded.last_channel_right, Some(11));
        assert_eq!(loaded.timer_minutes, Some(45));
    }
}

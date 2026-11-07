use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::i18n::Language;
use crate::project::MediaSourceType;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AppConfig {
    pub last_device_name: Option<String>,
    pub last_channel_left: Option<usize>,
    pub last_channel_right: Option<usize>,
    pub export_directory: Option<PathBuf>,
    pub timer_minutes: Option<u32>,
    pub time_headroom_minutes: Option<u32>,
    pub silence_autostop: Option<bool>,
    pub autostop_threshold_db: Option<f32>,
    pub autostop_silence_secs: Option<f32>,
    pub run_on_seconds: Option<u32>,
    pub pause_threshold_db: Option<f32>,
    pub pause_min_seconds: Option<f32>,
    pub pause_as_gaps: Option<bool>,
    pub zero_crossing_snap: Option<bool>,
    pub export_highpass_20hz: Option<bool>,
    pub export_fade_ms: Option<u32>,
    pub normalization_mode: Option<String>,
    pub language: Option<Language>,
    pub last_tape_name: Option<String>,
    pub last_artist_name: Option<String>,
    pub media_source: Option<MediaSourceType>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_device_name: None,
            last_channel_left: Some(10),  // Default to Zoom L-12 Kanäle 11/12 (0-indexed: 10/11)
            last_channel_right: Some(11),
            export_directory: None,
            timer_minutes: Some(45),
            time_headroom_minutes: Some(5),
            silence_autostop: Some(true),
            autostop_threshold_db: Some(-90.0),
            autostop_silence_secs: Some(20.0),
            run_on_seconds: Some(15),
            pause_threshold_db: Some(-45.0),
            pause_min_seconds: Some(1.8),
            pause_as_gaps: Some(false),
            zero_crossing_snap: Some(true),
            export_highpass_20hz: Some(true),
            export_fade_ms: Some(15),
            normalization_mode: Some("none".into()),
            language: Some(Language::De),
            last_tape_name: Some("Tonband 01".into()),
            last_artist_name: None,
            media_source: Some(MediaSourceType::Cassette),
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
        cfg.time_headroom_minutes = Some(5);
        let json = serde_json::to_string(&cfg).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.last_device_name.as_deref(), Some("ZOOM L-12 Driver"));
        assert_eq!(loaded.last_channel_left, Some(10));
        assert_eq!(loaded.last_channel_right, Some(11));
        assert_eq!(loaded.timer_minutes, Some(45));
        assert_eq!(loaded.time_headroom_minutes, Some(5));
        assert_eq!(loaded.autostop_threshold_db, Some(-90.0));
    }
}

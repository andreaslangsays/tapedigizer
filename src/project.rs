use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapeSide {
    SideA,
    SideB,
}

impl Default for TapeSide {
    fn default() -> Self {
        TapeSide::SideA
    }
}

impl TapeSide {
    pub fn label(&self, lang: crate::i18n::Language) -> &'static str {
        match (self, lang) {
            (TapeSide::SideA, crate::i18n::Language::De) => "SEITE A",
            (TapeSide::SideA, crate::i18n::Language::En) => "SIDE A",
            (TapeSide::SideB, crate::i18n::Language::De) => "SEITE B",
            (TapeSide::SideB, crate::i18n::Language::En) => "SIDE B",
        }
    }

    pub fn folder_name(&self, lang: crate::i18n::Language) -> &'static str {
        match (self, lang) {
            (TapeSide::SideA, crate::i18n::Language::De) => "Seite A",
            (TapeSide::SideA, crate::i18n::Language::En) => "Side A",
            (TapeSide::SideB, crate::i18n::Language::De) => "Seite B",
            (TapeSide::SideB, crate::i18n::Language::En) => "Side B",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TapeDigizerProject {
    pub app_version: String,
    pub tape_name: String,
    pub tape_side: TapeSide,
    pub audio_file_path: String,
    pub markers: Vec<f64>,
    pub custom_titles: Vec<String>,
    pub normalization_mode: String,
    pub created_at: String,
}

impl TapeDigizerProject {
    pub fn new(
        tape_name: String,
        tape_side: TapeSide,
        audio_file_path: String,
        markers: Vec<f64>,
        custom_titles: Vec<String>,
        normalization_mode: String,
    ) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tape_name,
            tape_side,
            audio_file_path,
            markers,
            custom_titles,
            normalization_mode,
            created_at: chrono_like_now(),
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        project_file_path: P,
        source_session_wav: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let proj_path = project_file_path.as_ref();
        let parent_dir = proj_path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent_dir).map_err(|e| e.to_string())?;

        let mut proj = self.clone();

        // Copy source WAV alongside project file so it is 100% self-contained
        if let Some(src_wav) = source_session_wav {
            if src_wav.exists() {
                let stem = proj_path.file_stem().and_then(|s| s.to_str()).unwrap_or("tape_audio");
                let companion_wav_name = format!("{}.wav", stem);
                let companion_wav_path = parent_dir.join(&companion_wav_name);

                if src_wav != companion_wav_path {
                    let _ = std::fs::copy(src_wav, &companion_wav_path);
                }
                proj.audio_file_path = companion_wav_name;
            }
        }

        let json = serde_json::to_string_pretty(&proj).map_err(|e| e.to_string())?;
        std::fs::write(proj_path, json).map_err(|e| e.to_string())?;
        Ok(proj_path.to_path_buf())
    }

    pub fn load_from_file<P: AsRef<Path>>(
        project_file_path: P,
    ) -> Result<(Self, PathBuf), String> {
        let proj_path = project_file_path.as_ref();
        let content = std::fs::read_to_string(proj_path).map_err(|e| e.to_string())?;
        let proj: Self = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let parent_dir = proj_path.parent().unwrap_or_else(|| Path::new("."));
        let audio_path = parent_dir.join(&proj.audio_file_path);
        let resolved_audio_path = if audio_path.exists() {
            audio_path
        } else {
            PathBuf::from(&proj.audio_file_path)
        };

        if !resolved_audio_path.exists() {
            return Err(format!(
                "Die zugehörige Audiodatei '{}' wurde nicht gefunden.",
                proj.audio_file_path
            ));
        }

        Ok((proj, resolved_audio_path))
    }
}

pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        "Tape_Album".to_string()
    } else {
        trimmed.to_string()
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("timestamp_{}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Sommer/Hits: 1985?"), "Sommer_Hits_ 1985_");
        assert_eq!(sanitize_filename("   "), "Tape_Album");
        assert_eq!(sanitize_filename("Tape 01"), "Tape 01");
    }

    #[test]
    fn test_tape_side_folder() {
        assert_eq!(TapeSide::SideA.folder_name(crate::i18n::Language::De), "Seite A");
        assert_eq!(TapeSide::SideA.folder_name(crate::i18n::Language::En), "Side A");
        assert_eq!(TapeSide::SideB.folder_name(crate::i18n::Language::De), "Seite B");
        assert_eq!(TapeSide::SideB.folder_name(crate::i18n::Language::En), "Side B");
    }

    #[test]
    fn test_project_save_load() {
        let temp_dir = std::env::temp_dir().join("tapedigizer_test_proj");
        let _ = std::fs::create_dir_all(&temp_dir);

        let test_wav = temp_dir.join("source.wav");
        std::fs::write(&test_wav, b"RIFFFAKEWAVDATA").unwrap();

        let proj = TapeDigizerProject::new(
            "Test Tape".into(),
            TapeSide::SideA,
            "".into(),
            vec![12.5, 45.0],
            vec!["Track 01".into(), "Track 02".into(), "Track 03".into()],
            "track".into(),
        );

        let proj_path = temp_dir.join("test_tape_Side_A.tapedigizer");
        proj.save_to_file(&proj_path, Some(&test_wav)).unwrap();

        assert!(proj_path.exists());
        let (loaded, audio_path) = TapeDigizerProject::load_from_file(&proj_path).unwrap();
        assert_eq!(loaded.tape_name, "Test Tape");
        assert_eq!(loaded.tape_side, TapeSide::SideA);
        assert_eq!(loaded.markers, vec![12.5, 45.0]);
        assert_eq!(loaded.custom_titles.len(), 3);
        assert!(audio_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaSourceType {
    Cassette,
    ReelToReel,
    VinylLp,
    VinylSingle,
}

impl Default for MediaSourceType {
    fn default() -> Self {
        MediaSourceType::Cassette
    }
}

impl MediaSourceType {
    pub fn label(&self, lang: crate::i18n::Language) -> &'static str {
        match (self, lang) {
            (MediaSourceType::Cassette, crate::i18n::Language::De) => "Kassette",
            (MediaSourceType::Cassette, crate::i18n::Language::En) => "Cassette",
            (MediaSourceType::ReelToReel, crate::i18n::Language::De) => "Tonband",
            (MediaSourceType::ReelToReel, crate::i18n::Language::En) => "Reel-to-Reel",
            (MediaSourceType::VinylLp, crate::i18n::Language::De) => "Vinyl LP",
            (MediaSourceType::VinylLp, crate::i18n::Language::En) => "Vinyl LP",
            (MediaSourceType::VinylSingle, crate::i18n::Language::De) => "Single 45",
            (MediaSourceType::VinylSingle, crate::i18n::Language::En) => "Single 45",
        }
    }

    /// Recommended default recording timer (minutes), autostop threshold (dB), autostop duration (s),
    /// pause detection threshold (dB), and minimum pause duration (s).
    pub fn defaults(&self) -> (u32, f32, f32, f32, f32) {
        match self {
            MediaSourceType::Cassette => (45, -90.0, 20.0, -48.0, 1.8),
            MediaSourceType::ReelToReel => (60, -90.0, 20.0, -50.0, 2.0),
            MediaSourceType::VinylLp => (25, -85.0, 15.0, -42.0, 1.4),
            MediaSourceType::VinylSingle => (10, -85.0, 12.0, -42.0, 1.2),
        }
    }

    /// Recommended default time headroom (buffer) in minutes.
    pub fn default_headroom_minutes(&self) -> u32 {
        5
    }
}

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
    #[serde(default)]
    pub pause_indices: Vec<usize>,
    #[serde(default)]
    pub disabled_filter_indices: Vec<usize>,
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
        pause_indices: Vec<usize>,
        disabled_filter_indices: Vec<usize>,
        normalization_mode: String,
    ) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tape_name,
            tape_side,
            audio_file_path,
            markers,
            custom_titles,
            pause_indices,
            disabled_filter_indices,
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
        if !proj_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("tapedigizer")) {
            return Err("Project destination must use the .tapedigizer extension".into());
        }
        let parent_dir = proj_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent_dir).map_err(|e| e.to_string())?;

        let mut proj = self.clone();
        let mut new_audio = None;
        if let Some(source) = source_session_wav {
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            if proj_path.canonicalize().ok().as_ref() == Some(&source) {
                return Err("The project destination points to its audio source".into());
            }
            let parent = parent_dir.canonicalize().map_err(|e| e.to_string())?;
            if source.parent() == Some(parent.as_path()) {
                proj.audio_file_path = source.file_name().unwrap().to_string_lossy().into_owned();
            } else {
                let (temporary, mut output) = unique_file(parent_dir, ".audio", "partial")?;
                let result = (|| {
                    let mut input = std::fs::File::open(&source)?;
                    std::io::copy(&mut input, &mut output)?;
                    output.sync_all()?;
                    // Publish a complete copy under a fresh name without replacing audio.
                    let stem = temporary.file_stem().unwrap().to_string_lossy();
                    let project_name = sanitize_filename(&proj_path.file_stem().unwrap_or_default().to_string_lossy());
                    let destination = parent_dir.join(format!("{project_name}{}.wav", stem.trim_start_matches(".audio")));
                    publish_new_file(&temporary, &destination)?;
                    Ok::<_, std::io::Error>(destination)
                })();
                let _ = std::fs::remove_file(&temporary);
                let destination = result.map_err(|e| e.to_string())?;
                proj.audio_file_path = destination.file_name().unwrap().to_string_lossy().into_owned();
                new_audio = Some(destination);
            }
        }

        let result = (|| {
            use std::io::Write;
            let json = serde_json::to_vec_pretty(&proj).map_err(|e| e.to_string())?;
            let (temporary, mut output) = unique_file(parent_dir, ".project", "partial")?;
            let write = output.write_all(&json).and_then(|_| output.sync_all());
            let commit = write.and_then(|_| std::fs::rename(&temporary, proj_path));
            let _ = std::fs::remove_file(&temporary);
            commit.map_err(|e| e.to_string())
        })();
        if result.is_err() {
            if let Some(path) = new_audio {
                let _ = std::fs::remove_file(path);
            }
        }
        result?;
        Ok(proj_path.to_path_buf())
    }

    pub fn load_from_file<P: AsRef<Path>>(
        project_file_path: P,
    ) -> Result<(Self, Option<PathBuf>), String> {
        let proj_path = project_file_path.as_ref();
        let content = std::fs::read_to_string(proj_path).map_err(|e| e.to_string())?;
        let proj: Self = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        if proj.audio_file_path.is_empty() {
            return Ok((proj, None));
        }

        let parent_dir = proj_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        let audio_path = parent_dir.join(&proj.audio_file_path);
        let resolved_audio_path = if audio_path.is_file() {
            audio_path
        } else {
            PathBuf::from(&proj.audio_file_path)
        };

        if !resolved_audio_path.is_file() {
            return Err(format!(
                "Die zugehörige Audiodatei '{}' wurde nicht gefunden.",
                proj.audio_file_path
            ));
        }

        Ok((proj, Some(resolved_audio_path)))
    }
}

/// Publish a complete file without overwriting an existing destination.
pub(crate) fn publish_new_file(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        // Both paths are NUL-terminated and live for the duration of the call.
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        if result == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    }
    #[cfg(not(target_os = "macos"))]
    std::fs::hard_link(from, to)
}

pub(crate) fn unique_file(dir: &Path, stem: &str, extension: &str) -> Result<(PathBuf, std::fs::File), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{stem}-{timestamp}-{sequence}.{extension}"));
    let file = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((path, file))
}

pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .filter_map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => Some('_'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.');
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "Tape_Album".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

pub fn sanitize_track_title(title: &str, fallback_index: usize) -> String {
    let sanitized: String = title
        .chars()
        .filter_map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => Some('_'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.');
    if trimmed.is_empty() {
        format!("Track {:02}", fallback_index)
    } else {
        trimmed.chars().take(80).collect()
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
    fn failed_save_keeps_existing_project_and_source() {
        let (file, handle) = unique_file(&std::env::temp_dir(), "tapedigizer-project-test", "tapedigizer").unwrap();
        drop(handle);
        std::fs::write(&file, b"original project").unwrap();
        let source = file.with_extension("wav");
        let project = TapeDigizerProject::new("Test".into(), TapeSide::SideA, "".into(), vec![], vec![], vec![], vec![], "none".into());
        assert!(project.save_to_file(&file, Some(&source)).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"original project");
        std::fs::write(&source, b"original audio").unwrap();
        assert!(project.save_to_file(&source, Some(&source)).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"original audio");
        std::fs::remove_file(file).unwrap();
        std::fs::remove_file(source).unwrap();
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
            vec![1],
            vec![2],
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
        assert_eq!(loaded.pause_indices, vec![1]);
        assert_eq!(loaded.disabled_filter_indices, vec![2]);
        assert!(audio_path.unwrap().is_file());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

use std::collections::BTreeMap;
use crate::audio::denoiser::{DenoiserParams, NoiseProfile};
use crate::audio::processing::SongSettings;
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub artist_name: String,
    pub media_source: MediaSourceType,
    pub export_highpass_20hz: bool,
    pub export_fade_ms: u32,
    pub export_balance_db: f32,
    pub export_apply_denoiser: bool,
    pub denoiser_params: DenoiserParams,
    pub noise_profile: Option<NoiseProfile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongEditorView {
    pub selected_segment: Option<usize>,
    pub zoom: f32,
    pub vertical_zoom: f32,
    pub scroll: f32,
    pub loop_playback: bool,
}

impl Default for SongEditorView {
    fn default() -> Self {
        Self { selected_segment: None, zoom: 1.0, vertical_zoom: 1.0, scroll: 0.0, loop_playback: false }
    }
}

const CURRENT_FORMAT_VERSION: u32 = 2;

fn legacy_format_version() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TapeDigizerProject {
    #[serde(default = "legacy_format_version")]
    pub format_version: u32,
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
    #[serde(default)]
    pub song_settings: BTreeMap<usize, SongSettings>,
    #[serde(default)]
    pub settings: Option<ProjectSettings>,
    #[serde(default)]
    pub editor_view: SongEditorView,
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
            format_version: CURRENT_FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            tape_name,
            tape_side,
            audio_file_path,
            markers,
            custom_titles,
            pause_indices,
            disabled_filter_indices,
            normalization_mode,
            song_settings: BTreeMap::new(),
            settings: None,
            editor_view: SongEditorView::default(),
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
        proj.format_version = CURRENT_FORMAT_VERSION;
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
    fn first_project_format_without_pause_or_filter_indices_loads() {
        let legacy = r#"{"app_version":"0.2.0","tape_name":"Archive","tape_side":"SideA",
            "audio_file_path":"archive.wav","markers":[3.0],"custom_titles":["One","Two"],
            "normalization_mode":"track","created_at":"then"}"#;
        let project: TapeDigizerProject = serde_json::from_str(legacy).unwrap();
        assert_eq!(project.format_version, 1);
        assert_eq!(project.audio_file_path, "archive.wav");
        assert_eq!(project.custom_titles, ["One", "Two"]);
        assert_eq!(project.markers, [3.0]);
        assert_eq!(project.normalization_mode, "track");
        assert!(project.pause_indices.is_empty());
        assert!(project.disabled_filter_indices.is_empty());
        assert!(project.settings.is_none() && project.song_settings.is_empty());
    }

    #[test]
    fn old_project_migrates_and_round_trips_complete_song_settings() {
        use crate::audio::processing::{DynamicsSettings, EqSettings, Routing, SongSettings};
        let (path, handle) = unique_file(&std::env::temp_dir(), "song-project", "tapedigizer").unwrap();
        drop(handle);
        std::fs::write(&path, r#"{"app_version":"1.6.0","tape_name":"Old","tape_side":"SideB",
            "audio_file_path":"","markers":[1.5,3.0],"custom_titles":["One","Gap","Two"],
            "pause_indices":[1],"disabled_filter_indices":[2],"normalization_mode":"album","created_at":"then"}"#).unwrap();
        let (mut project, _) = TapeDigizerProject::load_from_file(&path).unwrap();
        assert!(project.song_settings.is_empty());
        assert!(project.settings.is_none());
        assert_eq!(project.editor_view, SongEditorView::default());
        let profile = crate::audio::denoiser::NoiseProfile::learn_from_samples(
            &[0.125; 256],
            &[-0.25; 256],
            48_000,
            "Song noise",
            "Cassette",
        )
        .unwrap();
        let mut song = SongSettings {
            gain_db: -2.0,
            balance_db: -3.0,
            width: 0.78,
            swap_channels: true,
            export_mode: crate::audio::processing::SongExportMode::SeparateMono,
            routing: Routing::MonoMix,
            mix_left: 0.65,
            mix_right: 0.15,
            mono_gain_db: 1.5,
            channels_linked: false,
            stereo_dynamics_linked: false,
            bypass: true,
            legacy_highpass_20hz: true,
            noise_profile: Some(profile.clone()),
            ..SongSettings::default()
        };
        use crate::audio::processing::EffectStage;
        song.effect_chain.move_by(EffectStage::Limiter, -3);
        song.effect_chain.set_enabled(EffectStage::Equalizer, false);
        let channel = &mut song.channels[1];
        channel.gain_db = 3.0;
        channel.muted = true;
        channel.invert = true;
        channel.eq = EqSettings {
            enabled: true,
            low_db: -1.0,
            mid_db: 2.0,
            high_db: 3.0,
            low_hz: 110.0,
            mid_hz: 800.0,
            high_hz: 8000.0,
            mid_q: 0.9,
            peaks: [
                crate::audio::processing::PeakEqBand { hz: 300.0, gain_db: 1.0, q: 2.0 },
                crate::audio::processing::PeakEqBand { hz: 4200.0, gain_db: -2.0, q: 0.7 },
            ],
        };
        channel.denoiser = crate::audio::denoiser::DenoiserParams {
            enabled: true,
            dc_filter_enabled: true,
            dc_cutoff_hz: 30.0,
            reduction_db: 7.0,
            sensitivity: 2.2,
            smoothing: 0.8,
            ..Default::default()
        };
        channel.compressor = DynamicsSettings {
            enabled: true,
            threshold_db: -23.0,
            ratio: 3.0,
            attack_ms: 7.0,
            release_ms: 210.0,
            knee_db: 2.0,
            makeup_db: 1.0,
            upward: false,
        };
        channel.expander =
            DynamicsSettings { upward: true, threshold_db: -33.0, ..channel.compressor.clone() };
        project.settings = Some(ProjectSettings {
            artist_name: "Artist".into(),
            media_source: MediaSourceType::ReelToReel,
            export_highpass_20hz: true,
            export_fade_ms: 22,
            export_balance_db: 1.5,
            export_apply_denoiser: true,
            denoiser_params: channel.denoiser.clone(),
            noise_profile: Some(profile),
        });
        project.song_settings.insert(0, song);
        project.song_settings.insert(2, SongSettings::default());
        project.editor_view = SongEditorView {
            selected_segment: Some(2),
            zoom: 2.5,
            vertical_zoom: 1.5,
            scroll: 0.25,
            loop_playback: true,
        };
        project.save_to_file(&path, None).unwrap();
        let (reopened, _) = TapeDigizerProject::load_from_file(&path).unwrap();
        assert_eq!(project.format_version, 1);
        assert_eq!(reopened.format_version, 2);
        project.format_version = 2;
        assert_eq!(reopened, project);
        std::fs::remove_file(path).unwrap();
    }

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

        assert_eq!(proj.format_version, 2);
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

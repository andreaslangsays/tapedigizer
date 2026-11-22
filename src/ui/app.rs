use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::Arc;
use egui::{Pos2, RichText, Stroke, Ui, Vec2};
use crate::analysis::{analyze_samples, AudioAnalysisReport, LevelDiagnosis};
use crate::audio::denoiser::{
    AuditionMode, DenoiserParams, NoisePresetManager, NoiseProfile,
};
use crate::audio::device::{get_channel_pairs_for_device, list_input_devices, AudioDeviceInfo, ChannelPair};
use crate::audio::player::{AudioPlayer, DecodedAudio};
use crate::audio::recorder::{AudioRecorder, RecorderState};
use crate::audio::recording_edit::{EditedRecording, PreparedAudio, RecordingEdit};
use crate::audio::waveform::WaveformCache;
use crate::splitter::{
    export_tracks_with_options, ExportOptions, MarkerSnapshot, NormalizationMode, TrackSplitter, TransientPoint,
};
use crate::ui::meter_widget::{meter_readout, render_stereo_meter, render_vintage_vu_meter};
use crate::ui::tape_deck::draw_tape_transport_deck;
use crate::ui::theme::{
    apply_theme, install_macos_fonts, palette, theme_mode, ThemeMode,
    HEIGHT_COMPACT, HEIGHT_CONTROL, HEIGHT_PRIMARY,
};
use crate::ui::widgets::button::{
    action_button, bevel_button, bevel_button_sized, compact_button, compact_primary_button,
    mode_button, primary_button, segmented_control, transport_button, BevelStyle,
};
use crate::ui::widgets::paint::draw_recessed_well;
use crate::ui::widgets::panel::{
    bay_header, panel_row, panel_title, rack_columns, studio_panel, studio_wordmark, surface_bay,
    time_display, PanelStyle,
};
use crate::i18n::Language;
use crate::audio::processing::SongSettings;
use crate::project::{ProjectSettings, SongEditorView};
use super::jobs::{ExportProgress, PendingJob};
use super::song_editor::{SongEditor, SongEditorAction};
use crate::project::{sanitize_filename, MediaSourceType, TapeDigizerProject, TapeSide};
use crate::ui::waveform_widget::{render_stereo_waveform, EditTool};

#[derive(Clone, Copy, PartialEq)]
enum ConsoleTab {
    Input,
    Edit,
    Filter,
    Tracks,
    Export,
}

enum SongEffectAction { ToggleNoise, ToggleDc, LearnNoise }

enum JobResult {
    Open(OpenedProject),
    Recording(DecodedAudio, WaveformCache, AudioAnalysisReport),
    Export(usize, PathBuf),
    Pauses(MarkerSnapshot),
    Saved(PathBuf, Option<PathBuf>, SavedEdits),
    AudioEdited(EditedRecording),
    AudioHistory { audio: PreparedAudio, redo: bool },
}

enum ExportOutcome {
    Done { count: usize, folder: PathBuf },
    Cancelled,
    Failed(String),
}

struct OpenedProject {
    document: TapeDigizerProject,
    project_path: Option<PathBuf>,
    audio: Option<PreparedAudio>,
}

struct SavedEdits {
    name: String,
    side: TapeSide,
    markers: MarkerSnapshot,
    normalization: NormalizationMode,
    settings: ProjectSettings,
    editor_view: SongEditorView,
}

struct TransientCache {
    marker_time: f64,
    radius: f64,
    sensitivity: f32,
    points: std::sync::Arc<[TransientPoint]>,
}

pub struct TapeDigizerApp {
    // Internationalization & Appearance
    pub language: Language,

    // Tape & Album Organization
    pub media_source: MediaSourceType,
    pub tape_name: String,
    pub artist_name: String,
    pub tape_side: TapeSide,
    pub current_project_path: Option<PathBuf>,
    pub current_audio_path: Option<PathBuf>,

    // Deferred marker removal
    pending_delete_marker: Option<usize>,

    console_tab: ConsoleTab,

    // Audio Devices & Configuration
    available_devices: Vec<AudioDeviceInfo>,
    selected_device_idx: usize,
    channel_pairs: Vec<ChannelPair>,
    selected_pair_idx: usize,

    // Audio Engines
    recorder: AudioRecorder,
    player: AudioPlayer,
    song_editor: SongEditor,
    splitter: TrackSplitter,

    // Timer & Auto-Stop Settings
    selected_timer_minutes: u32,
    time_headroom_minutes: u32,
    enable_silence_autostop: bool,
    autostop_threshold_db: f32,
    autostop_silence_secs: f32,
    run_on_seconds: u32,

    // Pause Detection Options
    pause_threshold_db: f32,
    pause_min_seconds: f32,
    pause_as_gaps: bool,

    // Post-recording Analysis Report
    analysis_report: Option<AudioAnalysisReport>,

    // Stereo Waveform & Markers
    waveform_cache: WaveformCache,
    selected_marker_idx: Option<usize>,
    marker_drag: Option<MarkerSnapshot>,
    marker_drag_cancelled: bool,
    transient_cache: Option<TransientCache>,
    selected_segment_idx: Option<usize>,
    waveform_zoom: f32,
    waveform_vertical_zoom: f32,
    track_page: usize,
    export_page: usize,
    export_song: Option<usize>,
    selected_region: Option<(f64, f64)>,
    scope_radius_sec: f64,
    scope_locked_center: Option<f64>,
    zero_crossing_snap: bool,
    transient_sensitivity: f32,
    show_transients: bool,
    scope_zoom: f32,
    scope_gain_db: f32,
    scope_auto_gain: bool,
    scope_scroll: f32,
    waveform_scroll: f32,
    active_edit_tool: EditTool,
    batch_titles_open: bool,
    batch_titles_text: String,

    // Hover Audition (Stationary Mouse Preview)
    hover_preview_enabled: bool,
    hover_mouse_pos: Option<Pos2>,
    hover_stationary_since: Option<std::time::Instant>,
    hover_audition_time: Option<f64>,

    // Export & Mastering Options
    export_directory: PathBuf,
    normalization_mode: NormalizationMode,
    export_highpass_20hz: bool,
    export_fade_ms: u32,
    export_balance_db: f32,
    export_outcome: Option<ExportOutcome>,

    // Spectral Denoiser, Presets & Original Recording Cut
    denoiser_params: DenoiserParams,
    active_noise_profile: Option<NoiseProfile>,
    active_noise_samples: Option<(Vec<f32>, Vec<f32>, u32)>,
    saved_noise_presets: Vec<NoiseProfile>,
    selected_preset_idx: Option<usize>,
    preset_save_name: String,
    audio_status: Option<String>,
    export_apply_denoiser: bool,

    // General status/error
    ui_error_message: Option<String>,
    job: Option<PendingJob<JobResult>>,
    saved_edits: Option<SavedEdits>,
    pause_preview: Option<MarkerSnapshot>,
    #[cfg(target_os = "macos")]
    mac_menu: Option<super::macos_menu::MacMenu>,
}

impl TapeDigizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_macos_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, ThemeMode::Silverface);

        let cfg = crate::config::AppConfig::load();
        let devices = list_input_devices();
        let mut selected_dev_idx = 0;
        let mut dev_found = false;

        if let Some(ref saved_name) = cfg.last_device_name {
            if let Some(pos) = devices.iter().position(|d| &d.name == saved_name) {
                selected_dev_idx = pos;
                dev_found = true;
            }
        }

        if !dev_found {
            for (i, dev) in devices.iter().enumerate() {
                if dev.name.to_lowercase().contains("l-12") || dev.name.to_lowercase().contains("zoom") {
                    selected_dev_idx = i;
                    break;
                }
            }
        }

        let channel_pairs = if let Some(dev) = devices.get(selected_dev_idx) {
            get_channel_pairs_for_device(dev)
        } else {
            Vec::new()
        };

        let mut selected_pair_idx = 0;
        if let (Some(left), Some(right)) = (cfg.last_channel_left, cfg.last_channel_right) {
            if let Some(pos) = channel_pairs.iter().position(|p| p.left_channel == left && p.right_channel == right) {
                selected_pair_idx = pos;
            }
        }

        let default_export_dir = cfg
            .export_directory
            .filter(|p| p.exists())
            .or_else(dirs_default_music_dir)
            .unwrap_or_else(|| PathBuf::from("./TapeDigizer_Exports"));

        let media_source = cfg.media_source.unwrap_or(MediaSourceType::Cassette);
        let (def_timer, def_auto_db, def_auto_sec, def_pause_db, def_pause_sec) = media_source.defaults();

        let timer_minutes = cfg.timer_minutes.unwrap_or(def_timer);
        let time_headroom_minutes = cfg.time_headroom_minutes.unwrap_or(5);
        let silence_stop = cfg.silence_autostop.unwrap_or(true);
        let autostop_threshold_db = cfg.autostop_threshold_db.unwrap_or(def_auto_db);
        let autostop_silence_secs = cfg.autostop_silence_secs.unwrap_or(def_auto_sec);
        let run_on_seconds = cfg.run_on_seconds.unwrap_or(15);
        let pause_threshold_db = cfg.pause_threshold_db.unwrap_or(def_pause_db);
        let pause_min_seconds = cfg.pause_min_seconds.unwrap_or(def_pause_sec);
        let pause_as_gaps = cfg.pause_as_gaps.unwrap_or(false);
        let zero_crossing_snap = cfg.zero_crossing_snap.unwrap_or(true);
        let export_highpass_20hz = cfg.export_highpass_20hz.unwrap_or(true);
        let export_fade_ms = cfg.export_fade_ms.unwrap_or(15);

        let norm_mode = match cfg.normalization_mode.as_deref() {
            Some("track") => NormalizationMode::PeakPerTrack,
            Some("album") => NormalizationMode::PeakAlbum,
            _ => NormalizationMode::None,
        };

        let lang = cfg.language.unwrap_or(Language::De);
        let tape_name = cfg.last_tape_name.clone().unwrap_or_else(|| "Tonband 01".to_string());
        let artist_name = cfg.last_artist_name.clone().unwrap_or_default();

        let mut recorder = AudioRecorder::new();
        recorder.configure_autostop(autostop_threshold_db, autostop_silence_secs, run_on_seconds);

        let app = Self {
            language: lang,
            media_source,
            tape_name,
            artist_name,
            tape_side: TapeSide::SideA,
            current_project_path: None,
            current_audio_path: None,
            pending_delete_marker: None,
            console_tab: ConsoleTab::Input,
            available_devices: devices,
            selected_device_idx: selected_dev_idx,
            channel_pairs,
            selected_pair_idx,
            recorder,
            player: AudioPlayer::new(),
            song_editor: SongEditor::default(),
            splitter: TrackSplitter::new(),
            selected_timer_minutes: timer_minutes,
            time_headroom_minutes,
            enable_silence_autostop: silence_stop,
            autostop_threshold_db,
            autostop_silence_secs,
            run_on_seconds,
            pause_threshold_db,
            pause_min_seconds,
            pause_as_gaps,
            analysis_report: None,
            waveform_cache: WaveformCache::default(),
            selected_marker_idx: None,
            marker_drag: None,
            marker_drag_cancelled: false,
            transient_cache: None,
            selected_segment_idx: None,
            waveform_zoom: 1.0,
            waveform_vertical_zoom: 1.0,
            track_page: 0,
            export_page: 0,
            export_song: None,
            selected_region: None,
            scope_radius_sec: 1.5,
            scope_locked_center: None,
            zero_crossing_snap,
            transient_sensitivity: 1.0,
            show_transients: true,
            scope_zoom: 1.0,
            scope_gain_db: 0.0,
            scope_auto_gain: true,
            scope_scroll: 0.0,
            waveform_scroll: 0.0,
            active_edit_tool: EditTool::Pointer,
            batch_titles_open: false,
            batch_titles_text: String::new(),
            hover_preview_enabled: false,
            hover_mouse_pos: None,
            hover_stationary_since: None,
            hover_audition_time: None,
            export_directory: default_export_dir,
            normalization_mode: norm_mode,
            export_highpass_20hz,
            export_fade_ms,
            export_balance_db: 0.0,
            export_outcome: None,
            denoiser_params: DenoiserParams::default(),
            active_noise_profile: None,
            active_noise_samples: None,
            saved_noise_presets: NoisePresetManager::list_presets(),
            selected_preset_idx: None,
            preset_save_name: String::new(),
            audio_status: None,
            export_apply_denoiser: true,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            #[cfg(target_os = "macos")]
            mac_menu: None,
        };

        app.save_config();
        app
    }

    fn apply_media_source_preset(&mut self, source: MediaSourceType) {
        self.media_source = source;
        let (timer, auto_db, auto_sec, pause_db, pause_sec) = source.defaults();
        self.selected_timer_minutes = timer;
        self.time_headroom_minutes = source.default_headroom_minutes();
        self.autostop_threshold_db = auto_db;
        self.autostop_silence_secs = auto_sec;
        self.pause_threshold_db = pause_db;
        self.pause_min_seconds = pause_sec;
        self.recorder.configure_autostop(self.autostop_threshold_db, self.autostop_silence_secs, self.run_on_seconds);
        self.save_config();
    }

    fn snap_time_if_enabled(&self, seconds: f64) -> f64 {
        if self.zero_crossing_snap && self.player.sample_rate() > 0 {
            let l = self.player.get_samples_l();
            let r = self.player.get_samples_r();
            if !l.is_empty() {
                return crate::splitter::snap_to_zero_crossing(&l, &r, self.player.sample_rate(), seconds, 2.5);
            }
        }
        seconds
    }

    fn preview_marker_drag(&mut self, index: usize, time: f64) {
        if self.marker_drag_cancelled {
            return;
        }
        self.marker_drag.get_or_insert_with(|| self.splitter.snapshot());
        self.selected_marker_idx = Some(index);
        self.selected_segment_idx = Some(index + 1);
        let snapped = self.snap_time_if_enabled(time);
        self.splitter.preview_marker_move(index, snapped, self.player.total_seconds(), self.player.sample_rate());
    }

    fn nudge_marker(&mut self, index: usize, delta: f64, duration: f64) {
        if let Some(&time) = self.splitter.markers.get(index) {
            let target = self.snap_time_if_enabled(time + delta);
            self.splitter.move_marker(index, target, duration, self.player.sample_rate());
            self.scope_locked_center = None;
        }
    }

    fn reset_editor_selection(&mut self) {
        self.clear_hover_audition();
        self.audio_status = None;
        self.selected_marker_idx = None;
        self.selected_segment_idx = None;
        self.pending_delete_marker = None;
        self.marker_drag = None;
        self.selected_region = None;
        self.scope_locked_center = None;
        self.pause_preview = None;
        self.batch_titles_open = false;
        self.export_outcome = None;
        self.transient_cache = None;
        self.track_page = 0;
        self.waveform_zoom = 1.0;
        self.waveform_scroll = 0.0;
        self.scope_zoom = 1.0;
        self.scope_scroll = 0.0;
    }

    pub fn save_config(&self) {
        let dev_name = self.current_device().map(|d| d.name.clone());
        let pair = self.current_channel_pair();
        let norm_str = match self.normalization_mode {
            NormalizationMode::None => "none",
            NormalizationMode::PeakPerTrack => "track",
            NormalizationMode::PeakAlbum => "album",
        };

        let cfg = crate::config::AppConfig {
            last_device_name: dev_name,
            last_channel_left: pair.as_ref().map(|p| p.left_channel),
            last_channel_right: pair.as_ref().map(|p| p.right_channel),
            export_directory: Some(self.export_directory.clone()),
            timer_minutes: Some(self.selected_timer_minutes),
            time_headroom_minutes: Some(self.time_headroom_minutes),
            silence_autostop: Some(self.enable_silence_autostop),
            autostop_threshold_db: Some(self.autostop_threshold_db),
            autostop_silence_secs: Some(self.autostop_silence_secs),
            run_on_seconds: Some(self.run_on_seconds),
            pause_threshold_db: Some(self.pause_threshold_db),
            pause_min_seconds: Some(self.pause_min_seconds),
            pause_as_gaps: Some(self.pause_as_gaps),
            zero_crossing_snap: Some(self.zero_crossing_snap),
            export_highpass_20hz: Some(self.export_highpass_20hz),
            export_fade_ms: Some(self.export_fade_ms),
            normalization_mode: Some(norm_str.to_string()),
            language: Some(self.language),
            last_tape_name: Some(self.tape_name.clone()),
            last_artist_name: Some(self.artist_name.clone()),
            media_source: Some(self.media_source),
        };
        cfg.save();
    }

    pub fn effective_export_directory(&self) -> PathBuf {
        let tape_dir = sanitize_filename(&self.tape_name);
        let side_dir = self.tape_side.folder_name(self.language);
        self.export_directory.join(tape_dir).join(side_dir)
    }

    pub fn session_project_dir(&self) -> PathBuf {
        let tape_safe = sanitize_filename(&self.tape_name);
        self.export_directory.join(tape_safe)
    }

    fn next_recording_path(&self) -> Result<PathBuf, String> {
        let folder = self.session_project_dir();
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?.as_millis();
        let tape = sanitize_filename(&self.tape_name);
        let side = self.tape_side.folder_name(Language::En);
        Ok(folder.join(format!("{tape}_{side}_{stamp}.wav")))
    }

    fn text(&self, german: &'static str, english: &'static str) -> &'static str {
        if self.language == Language::De { german } else { english }
    }

    fn start_job(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        work: impl FnOnce() -> Result<JobResult, String> + Send + 'static,
    ) {
        if self.job.is_some() || self.recorder.is_busy() { return; }
        match PendingJob::start(ctx, label, work) {
            Ok(job) => {
                self.job = Some(job);
                self.ui_error_message = None;
            }
            Err(error) => self.ui_error_message = Some(error.to_string()),
        }
    }

    #[cfg(target_os = "macos")]
    fn handle_macos_menu(&mut self, ctx: &egui::Context) {
        use super::macos_menu::{Command, MacMenu};
        if self.mac_menu.as_ref().is_none_or(|menu| menu.language != self.language) {
            self.mac_menu = None;
            self.mac_menu = MacMenu::install(ctx, self.language);
        }
        let Some(menu) = &self.mac_menu else { return; };
        for command in menu.take_commands() {
            let editing_blocked = self.job.is_some() || self.recorder.is_busy();
            if self.visible_tab() == ConsoleTab::Filter && !editing_blocked {
                match command {
                    Command::ToggleLiveFilter | Command::ToggleSelectedTrackFilter => { self.apply_song_effect_action(SongEffectAction::ToggleNoise); continue; }
                    Command::ToggleSubsonicFilter => { self.apply_song_effect_action(SongEffectAction::ToggleDc); continue; }
                    Command::LearnNoise => { self.apply_song_effect_action(SongEffectAction::LearnNoise); continue; }
                    Command::BakeFilterTrack | Command::BakeFilterAll | Command::FilterAllTracks | Command::FilterNoTracks => {
                        self.ui_error_message = Some(self.text("Im Song-Studio werden Effekte pro Song gespeichert und beim Export angewendet.", "Song studio saves effects per song and applies them on export.").into());
                        continue;
                    }
                    _ => {}
                }
            }
            match command {
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Command::New => self.new_project(),
                Command::Open => self.open_project_dialog(ctx),
                Command::Save => self.save_project_quick(ctx),
                Command::SaveAs => self.save_project_dialog(ctx),
                Command::Reveal => self.reveal_session_in_finder(),
                Command::Undo | Command::Redo if !editing_blocked => {
                    let redo = matches!(command, Command::Redo);
                    if ctx.wants_keyboard_input() {
                        let modifiers = egui::Modifiers { command: true, mac_cmd: true, shift: redo, ..Default::default() };
                        ctx.input_mut(|input| input.events.push(egui::Event::Key {
                            key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers,
                        }));
                    } else {
                        self.undo_marker_edit(ctx, redo);
                    }
                }
                Command::Record => self.console_tab = ConsoleTab::Input,
                Command::Edit => self.console_tab = ConsoleTab::Edit,
                Command::Export => {
                    self.export_song = None;
                    self.console_tab = ConsoleTab::Export;
                }
                Command::FocusFilter => self.console_tab = ConsoleTab::Filter,
                Command::FocusScope => self.console_tab = ConsoleTab::Edit,
                Command::FocusTracks => self.console_tab = ConsoleTab::Tracks,
                Command::ToggleLiveFilter if !editing_blocked => {
                    self.denoiser_params.enabled = !self.denoiser_params.enabled;
                    self.player.set_denoiser_params(self.denoiser_params.clone());
                }
                Command::LearnNoise if !editing_blocked && self.player.total_samples() > 0 => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let (s0, s1) = self.active_selection_or_window(total_sec, cur_sec);
                    self.learn_noise_from_region(s0, s1);
                }
                Command::BakeFilterTrack if !editing_blocked && self.player.total_samples() > 0 => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let tracks = self.splitter.get_tracks(total_sec);
                    let seg_idx = self.selected_segment_idx.unwrap_or_else(|| {
                        tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds)
                            .map(|t| t.segment_index).unwrap_or(0)
                    });
                    self.bake_denoiser_into_track(ctx, seg_idx);
                }
                Command::BakeFilterAll if !editing_blocked && self.player.total_samples() > 0 => {
                    self.bake_denoiser_into_recording(ctx);
                }
                Command::FilterAllTracks if !editing_blocked => {
                    let total_sec = self.player.total_seconds();
                    let count = self.splitter.get_tracks(total_sec).len();
                    self.splitter.set_all_tracks_filtered(count, true);
                }
                Command::FilterNoTracks if !editing_blocked => {
                    let total_sec = self.player.total_seconds();
                    let count = self.splitter.get_tracks(total_sec).len();
                    self.splitter.set_all_tracks_filtered(count, false);
                }
                Command::ToggleSelectedTrackFilter if !editing_blocked => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let tracks = self.splitter.get_tracks(total_sec);
                    let seg_idx = self.selected_segment_idx.unwrap_or_else(|| {
                        tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds)
                            .map(|t| t.segment_index).unwrap_or(0)
                    });
                    self.splitter.toggle_track_filter(seg_idx);
                }
                Command::ToggleSubsonicFilter if !editing_blocked => {
                    self.export_highpass_20hz = !self.export_highpass_20hz;
                    self.save_config();
                }
                _ => {}
            }
        }
    }

    fn undo_marker_edit(&mut self, ctx: &egui::Context, redo: bool) {
        if self.job.is_some() || self.marker_drag.is_some() || self.recorder.is_busy() {
            return;
        }
        self.song_editor.finish_edit(&mut self.splitter);
        let Some(target) = self.splitter.history_target(redo) else { return; };
        if target.audio_revision != self.splitter.audio_revision {
            let Some(path) = target.audio_revision.clone() else { return; };
            self.splitter.revision_preview_enabled = self.denoiser_params.enabled;
            self.pause_audio_for_edit();
            self.start_job(ctx, self.text("Audioversion wird geladen …", "Loading audio revision …"), move || {
                let audio = PreparedAudio::read(path)?;
                Ok(JobResult::AudioHistory { audio, redo })
            });
        } else {
            self.apply_history_step(redo);
        }
    }

    fn apply_history_step(&mut self, redo: bool) -> bool {
        let changed = if redo { self.splitter.redo() } else { self.splitter.undo() };
        if changed {
            self.selected_marker_idx = None;
            self.selected_segment_idx = None;
            self.pending_delete_marker = None;
        }
        changed
    }

    fn with_recording_output(&mut self, play: impl FnOnce(&AudioPlayer)) -> bool {
        if self.player.total_samples() == 0 {
            return false;
        }
        if let Err(error) = self.player.ensure_output() {
            self.ui_error_message = Some(error);
            return false;
        }
        play(&self.player);
        true
    }

    fn play_recording_range(&mut self, start: f64, end: f64) {
        self.with_recording_output(|player| player.play_range(start, end));
    }

    fn pause_audio_for_edit(&mut self) {
        self.player.pause();
        self.clear_hover_audition();
        self.song_editor.suspend();
    }

    fn restore_recording_preview(&mut self) {
        self.denoiser_params.enabled = self.splitter.revision_preview_enabled;
        self.player.set_denoiser_params(self.denoiser_params.clone());
    }

    fn install_recording_revision(&mut self, audio: PreparedAudio) {
        self.player.load_decoded(audio.decoded);
        self.current_audio_path = Some(audio.path);
        self.waveform_cache = audio.waveform;
        self.analysis_report = Some(audio.report);
        self.reset_editor_selection();
        let mut view = self.song_editor.view.clone();
        view.selected_segment = view.selected_segment.filter(|&index| {
            index <= self.splitter.markers.len() && !self.splitter.is_pause(index)
        });
        self.selected_segment_idx = view.selected_segment;
        self.song_editor = SongEditor::default();
        self.song_editor.view = view;
        self.export_song = None;
    }

    fn poll_job(&mut self) {
        let Some(job) = &self.job else { return; };
        let interrupted = self.text(
            "Hintergrundaufgabe wurde unterbrochen.", "Background job stopped unexpectedly.",
        );
        let Some(result) = job.poll(interrupted) else { return; };
        let export = job.export_progress.is_some();
        self.job = None;
        match result {
            Ok(JobResult::Open(opened)) => {
                let project = opened.document;
                if let Some(audio) = opened.audio {
                    self.player.load_decoded(audio.decoded);
                    self.current_audio_path = Some(audio.path);
                    self.waveform_cache = audio.waveform;
                    self.analysis_report = Some(audio.report);
                } else {
                    self.player.reset();
                    self.current_audio_path = None;
                    self.waveform_cache = WaveformCache::default();
                    self.analysis_report = None;
                }
                self.tape_name = project.tape_name;
                self.tape_side = project.tape_side;
                self.splitter = TrackSplitter::new();
                self.splitter.restore(MarkerSnapshot {
                    audio_revision: self.current_audio_path.clone(),
                    revision_preview_enabled: false,
                    markers: project.markers,
                    custom_titles: project.custom_titles,
                    pause_indices: project.pause_indices,
                    disabled_filter_indices: project.disabled_filter_indices,
                    song_settings: project.song_settings,
                });
                self.normalization_mode = match project.normalization_mode.as_str() {
                    "track" => NormalizationMode::PeakPerTrack,
                    "album" => NormalizationMode::PeakAlbum,
                    _ => NormalizationMode::None,
                };
                if let Some(settings) = project.settings {
                    self.artist_name = settings.artist_name;
                    self.media_source = settings.media_source;
                    self.export_highpass_20hz = settings.export_highpass_20hz;
                    self.export_fade_ms = settings.export_fade_ms;
                    self.export_balance_db = settings.export_balance_db;
                    self.export_apply_denoiser = settings.export_apply_denoiser;
                    self.denoiser_params = settings.denoiser_params;
                    self.active_noise_profile = settings.noise_profile;
                } else {
                    // Older documents never stored restoration state. Use the
                    // legacy export defaults, never a previously opened song's profile.
                    let config = crate::config::AppConfig::load();
                    self.artist_name.clear();
                    self.export_balance_db = 0.0;
                    self.export_highpass_20hz = config.export_highpass_20hz.unwrap_or(true);
                    self.export_fade_ms = config.export_fade_ms.unwrap_or(15);
                    self.export_apply_denoiser = true;
                    self.denoiser_params = DenoiserParams::default();
                    self.active_noise_profile = None;
                }
                self.active_noise_samples = None;
                self.player.set_denoiser_params(self.denoiser_params.clone());
                self.player.set_noise_profile(self.active_noise_profile.clone());
                self.current_project_path = opened.project_path;
                self.export_song = None;
                self.export_outcome = None;
                self.reset_editor_selection();
                self.song_editor = SongEditor::default();
                self.song_editor.view = project.editor_view;
                let tracks = self.splitter.get_tracks(self.player.total_seconds());
                self.song_editor.view.selected_segment = tracks.iter()
                    .find(|track| !track.is_pause && Some(track.segment_index) == self.song_editor.view.selected_segment)
                    .or_else(|| tracks.iter().find(|track| !track.is_pause))
                    .map(|track| track.segment_index);
                self.selected_segment_idx = self.song_editor.view.selected_segment;
                self.saved_edits = self.current_project_path.as_ref().map(|_| self.edits());
                self.console_tab = ConsoleTab::Edit;
            }
            Ok(JobResult::Recording(audio, waveform, report)) => {
                self.console_tab = ConsoleTab::Edit;
                self.waveform_cache = waveform;
                self.analysis_report = Some(report);
                self.player.load_decoded(audio);
            }
            Ok(JobResult::Export(count, folder)) => {
                self.export_outcome = Some(ExportOutcome::Done { count, folder });
            }
            Ok(JobResult::Pauses(snapshot)) => self.pause_preview = Some(snapshot),
            Ok(JobResult::Saved(path, audio_path, edits)) => {
                self.apply_saved_project(path, audio_path, edits);
            }
            Ok(JobResult::AudioEdited(mut edited)) => {
                self.splitter.revision_preview_enabled = self.denoiser_params.enabled;
                let before = self.splitter.snapshot();
                edited.markers.revision_preview_enabled =
                    before.revision_preview_enabled && !edited.disable_live_denoiser;
                self.splitter.restore(edited.markers);
                self.splitter.commit_edit(before);
                self.restore_recording_preview();
                self.install_recording_revision(edited.audio);
                self.audio_status = Some(self.text(
                    "Bearbeitete WAV-Version angelegt",
                    "Created edited WAV revision",
                ).into());
            }
            Ok(JobResult::AudioHistory { audio, redo }) => {
                if self.apply_history_step(redo) {
                    self.restore_recording_preview();
                    self.install_recording_revision(audio);
                    self.audio_status = Some(self.text("Audioversion gewechselt", "Switched audio revision").into());
                }
            }
            Err(error) if export => {
                self.export_outcome = Some(if error == "Export cancelled" {
                    ExportOutcome::Cancelled
                } else {
                    ExportOutcome::Failed(error)
                });
            }
            Err(error) => self.ui_error_message = Some(error),
        }
    }

    fn edits(&self) -> SavedEdits {
        let mut markers = self.splitter.snapshot();
        markers.custom_titles.truncate(titles_without_padding(&markers.custom_titles).len());
        SavedEdits { name: self.tape_name.clone(), side: self.tape_side, markers,
            normalization: self.normalization_mode.clone(), settings: self.project_settings(), editor_view: self.song_editor.view.clone() }
    }

    fn has_unsaved_edits(&self) -> bool {
        let Some(saved) = &self.saved_edits else { return true; };
        let settings = &saved.settings;
        let markers = &saved.markers;
        let current = &self.splitter;
        // The footer is rendered while audio plays. Compare the current state
        // by reference instead of cloning every track and its noise profile.
        saved.name != self.tape_name
            || saved.side != self.tape_side
            || saved.normalization != self.normalization_mode
            || saved.editor_view != self.song_editor.view
            || settings.artist_name != self.artist_name
            || settings.media_source != self.media_source
            || settings.export_highpass_20hz != self.export_highpass_20hz
            || settings.export_fade_ms != self.export_fade_ms
            || settings.export_balance_db != self.export_balance_db
            || settings.export_apply_denoiser != self.export_apply_denoiser
            || settings.denoiser_params != self.denoiser_params
            || settings.noise_profile != self.active_noise_profile
            || markers.audio_revision != current.audio_revision
            || markers.markers != current.markers
            || markers.pause_indices != current.pause_indices
            || markers.disabled_filter_indices != current.disabled_filter_indices
            || markers.custom_titles != titles_without_padding(&current.custom_titles)
            || markers.song_settings != current.song_settings
    }

    fn project_settings(&self) -> ProjectSettings {
        ProjectSettings {
            artist_name: self.artist_name.clone(), media_source: self.media_source,
            export_highpass_20hz: self.export_highpass_20hz, export_fade_ms: self.export_fade_ms,
            export_balance_db: self.export_balance_db, export_apply_denoiser: self.export_apply_denoiser,
            denoiser_params: self.denoiser_params.clone(), noise_profile: self.active_noise_profile.clone(),
        }
    }

    fn legacy_song_settings(&self) -> SongSettings {
        let mut settings = SongSettings { balance_db: self.export_balance_db,
            noise_profile: self.active_noise_profile.clone(), ..Default::default() };
        for channel in &mut settings.channels {
            channel.denoiser = self.denoiser_params.clone();
            channel.denoiser.enabled = self.export_apply_denoiser && self.active_noise_profile.is_some();
            channel.denoiser.dc_filter_enabled = channel.denoiser.enabled && self.denoiser_params.dc_filter_enabled;
            channel.denoiser.audition_mode = AuditionMode::Wet;
        }
        settings.legacy_highpass_20hz = self.export_highpass_20hz && !settings.channels[0].denoiser.dc_filter_enabled;
        settings
    }

    fn apply_song_effect_action(&mut self, action: SongEffectAction) {
        let tracks = self.splitter.get_tracks(self.player.total_seconds());
        let Some(track) = self.song_editor.view.selected_segment.or(self.selected_segment_idx)
            .and_then(|idx| tracks.iter().find(|track| track.segment_index == idx && !track.is_pause)) else { return; };
        let mut settings = self.splitter.song_settings.get(&track.segment_index).cloned().unwrap_or_else(|| {
            let mut inherited = self.legacy_song_settings();
            if !track.is_filtered {
                inherited.legacy_highpass_20hz = self.export_highpass_20hz;
                for channel in &mut inherited.channels { channel.denoiser.enabled = false; channel.denoiser.dc_filter_enabled = false; }
            }
            inherited
        });
        let previous = settings.clone();
        match action {
            SongEffectAction::ToggleNoise => {
                let enabled = !settings.channels.iter().any(|channel| channel.denoiser.enabled);
                for channel in &mut settings.channels { channel.denoiser.enabled = enabled; }
            }
            SongEffectAction::ToggleDc => {
                let enabled = !settings.channels.iter().any(|channel| channel.denoiser.dc_filter_enabled);
                for channel in &mut settings.channels { channel.denoiser.dc_filter_enabled = enabled; }
            }
            SongEffectAction::LearnNoise => self.song_editor.learn_noise(&mut settings, &self.player.get_samples_l(), &self.player.get_samples_r(), self.player.sample_rate(), track),
        }
        if settings != previous {
            self.splitter.push_undo();
            self.splitter.song_settings.insert(track.segment_index, settings);
        }
    }

    fn can_leave_project(&mut self) -> bool {
        if self.recorder.is_busy() || self.job.is_some() {
            self.ui_error_message = Some(self.text(
                "Bitte beende zuerst die Aufnahme oder warte auf die laufende Aufgabe.",
                "Stop recording or wait for the current task to finish.",
            ).into());
            return false;
        }
        if (self.current_audio_path.is_none() && self.current_project_path.is_none())
            || !self.has_unsaved_edits()
        {
            return true;
        }
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(self.text("Ungespeicherte Änderungen", "Unsaved changes"))
            .set_description(self.text("Projekt ohne Speichern verlassen?", "Leave this project without saving?"))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show() == rfd::MessageDialogResult::Yes
    }

    pub fn save_project_quick(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.current_project_path.clone() {
            self.save_project_to_path(ctx, &path);
        } else {
            self.save_project_dialog(ctx);
        }
    }

    fn project_document(&self) -> TapeDigizerProject {
        let norm = match self.normalization_mode {
            NormalizationMode::None => "none",
            NormalizationMode::PeakPerTrack => "track",
            NormalizationMode::PeakAlbum => "album",
        };
        let mut document = TapeDigizerProject::new(
            self.tape_name.clone(),
            self.tape_side,
            String::new(),
            self.splitter.markers.clone(),
            self.splitter.custom_titles.clone(),
            self.splitter.pause_indices.clone(),
            self.splitter.disabled_filter_indices.clone(),
            norm.into(),
        );
        document.song_settings = self.splitter.song_settings.clone();
        document.settings = Some(self.project_settings());
        document.editor_view = self.song_editor.view.clone();
        document
    }

    fn apply_saved_project(&mut self, path: PathBuf, audio: Option<PathBuf>, mut edits: SavedEdits) {
        self.ui_error_message = None;
        if let Some(audio) = audio {
            if self.splitter.audio_revision.as_deref() != Some(audio.as_path()) {
                let previous = self.splitter.audio_revision.clone();
                self.splitter.relocate_audio_revision(&audio);
                edits.markers.relocate_audio_revision(previous.as_deref(), &audio);
                for snapshot in self.pause_preview.iter_mut().chain(self.marker_drag.iter_mut()) {
                    snapshot.relocate_audio_revision(previous.as_deref(), &audio);
                }
            }
            self.current_audio_path = Some(audio);
        }

        self.saved_edits = Some(edits);
        self.current_project_path = Some(path);
    }

    pub fn save_project_to_path(&mut self, ctx: &egui::Context, path: &Path) {
        if self.recorder.is_busy() || self.job.is_some() { return; }
        let project = self.project_document();
        let source = self.current_audio_path.clone();
        let edits = self.edits();
        let path = path.to_path_buf();
        self.start_job(ctx, self.text("Projekt wird gespeichert …", "Saving project …"), move || {
            let saved = project.save_to_file(&path, source.as_deref())?;
            let audio = if source.is_some() {
                TapeDigizerProject::load_from_file(&saved)?.1
            } else {
                None
            };
            Ok(JobResult::Saved(saved, audio, edits))
        });
    }

    pub fn new_project(&mut self) {
        if !self.can_leave_project() { return; }
        self.player.reset();
        self.recorder.stop_monitoring();
        self.reset_editor_selection();
        self.pause_preview = None;
        self.saved_edits = None;
        self.tape_name = if self.language == Language::De { "Neues Tonband".into() } else { "New Tape".into() };
        self.tape_side = TapeSide::SideA;
        self.current_project_path = None;
        self.current_audio_path = None;
        self.analysis_report = None;
        self.waveform_cache = WaveformCache::default();
        self.splitter = TrackSplitter::new();
        self.song_editor = SongEditor::default();
        self.export_song = None;
        self.ui_error_message = None;
        self.console_tab = ConsoleTab::Input;
    }

    pub fn reveal_session_in_finder(&self) {
        let path_to_open = if let Some(ref p) = self.current_project_path {
            p.parent().unwrap_or(&self.export_directory).to_path_buf()
        } else {
            let session_dir = self.session_project_dir();
            if session_dir.exists() {
                session_dir
            } else {
                self.export_directory.clone()
            }
        };
        let _ = std::fs::create_dir_all(&path_to_open);
        let _ = std::process::Command::new("open").arg(&path_to_open).spawn();
    }

    pub fn save_project_dialog(&mut self, ctx: &egui::Context) {
        if self.recorder.is_busy() || self.job.is_some() { return; }
        let suggested_filename = format!(
            "{}_{}.tapedigizer",
            sanitize_filename(&self.tape_name),
            self.tape_side.folder_name(Language::En)
        );
        let mut dialog = rfd::FileDialog::new()
            .set_file_name(&suggested_filename)
            .add_filter("TapeDigizer Project", &["tapedigizer"]);

        if let Some(ref current) = self.current_project_path {
            if let Some(parent) = current.parent() {
                dialog = dialog.set_directory(parent);
            }
        } else {
            let session_dir = self.session_project_dir();
            if session_dir.exists() {
                dialog = dialog.set_directory(&session_dir);
            } else {
                dialog = dialog.set_directory(&self.export_directory);
            }
        }

        if let Some(path) = dialog.save_file() {
            let path = if path.extension().is_none() { path.with_extension("tapedigizer") } else { path };
            self.save_project_to_path(ctx, &path);
        }
    }

    pub fn open_project_dialog(&mut self, ctx: &egui::Context) {
        if !self.can_leave_project() { return; }
        let dialog = rfd::FileDialog::new()
            .add_filter("TapeDigizer / WAV", &["tapedigizer", "wav"]);
        if let Some(path) = dialog.pick_file() {
            self.recorder.stop_monitoring();
            self.start_job(ctx, self.text("Projekt wird geladen …", "Opening project …"), move || {
                let (mut document, project_path, source) = if path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
                {
                    let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    let document = TapeDigizerProject::new(
                        name, TapeSide::SideA, String::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), "none".into(),
                    );
                    (document, None, Some(path))
                } else {
                    let (document, source) = TapeDigizerProject::load_from_file(&path)?;
                    (document, Some(path), source)
                };
                let audio = if let Some(path) = source {
                    let audio = PreparedAudio::read(path)?;
                    let decoded = &audio.decoded;
                    let mut last = 0;
                    for marker in &mut document.markers {
                        let frame = (*marker * f64::from(decoded.sample_rate)).round() as usize;
                        if !marker.is_finite() || frame <= last || frame >= decoded.left.len() {
                            return Err("Invalid or overlapping project markers".into());
                        }
                        *marker = frame as f64 / f64::from(decoded.sample_rate);
                        last = frame;
                    }
                    document.pause_indices.retain(|&index| index <= document.markers.len());
                    document.disabled_filter_indices.retain(|&index| index <= document.markers.len());
                    document.song_settings.retain(|&index, _| index <= document.markers.len());
                    Some(audio)
                } else {
                    None
                };
                Ok(JobResult::Open(OpenedProject { document, project_path, audio }))
            });
        }
    }

    fn refresh_device_list(&mut self) {
        self.available_devices = list_input_devices();
        let cfg = crate::config::AppConfig::load();

        if let Some(ref saved_name) = cfg.last_device_name {
            if let Some(pos) = self.available_devices.iter().position(|d| &d.name == saved_name) {
                self.selected_device_idx = pos;
            } else if self.selected_device_idx >= self.available_devices.len() {
                self.selected_device_idx = 0;
            }
        } else if self.selected_device_idx >= self.available_devices.len() {
            self.selected_device_idx = 0;
        }

        if let Some(dev) = self.available_devices.get(self.selected_device_idx) {
            self.channel_pairs = get_channel_pairs_for_device(dev);
            let mut pair_idx = 0;
            if let (Some(left), Some(right)) = (cfg.last_channel_left, cfg.last_channel_right) {
                if let Some(pos) = self.channel_pairs.iter().position(|p| p.left_channel == left && p.right_channel == right) {
                    pair_idx = pos;
                }
            }
            self.selected_pair_idx = pair_idx;
        }
    }

    fn current_device(&self) -> Option<&AudioDeviceInfo> {
        self.available_devices.get(self.selected_device_idx)
    }

    fn current_channel_pair(&self) -> Option<ChannelPair> {
        self.channel_pairs.get(self.selected_pair_idx).cloned()
    }

    fn finish_recording_and_analyze(&mut self, ctx: &egui::Context) {
        let Some(target) = self.recorder.current_filepath() else { return; };
        if let Some(error) = self.recorder.take_recording_error() {
            self.ui_error_message = Some(format!("{}: {error} · {}", self.text(
                "Aufnahme unterbrochen. Rohdatei", "Recording interrupted. Raw file",
            ), target.display()));
            return;
        }

        self.player.reset();
        self.splitter = TrackSplitter::new();
        self.reset_editor_selection();
        self.waveform_cache = WaveformCache::default();
        self.analysis_report = None;
        self.pause_preview = None;
        self.current_audio_path = Some(target.clone());
        self.splitter.audio_revision = Some(target.clone());
        self.current_project_path = None;
        self.saved_edits = None;
        let project_path = target.with_extension("tapedigizer");
        let save_error = match self.project_document().save_to_file(&project_path, Some(&target)) {
            Ok(path) => {
                self.apply_saved_project(path, Some(target.clone()), self.edits());
                None
            }
            Err(error) => Some(format!("{}: {error} · {}", self.text(
                "Projekt konnte nicht gespeichert werden. Rohdatei", "Could not save project. Raw file",
            ), target.display())),
        };
        self.start_job(ctx, self.text("Aufnahme wird analysiert …", "Analyzing recording …"), move || {
            let audio = AudioPlayer::decode_file(&target)?;
            let waveform = WaveformCache::build(&audio);
            let report = analyze_samples(&audio.left, &audio.right, audio.sample_rate)?;
            Ok(JobResult::Recording(audio, waveform, report))
        });
        if save_error.is_some() { self.ui_error_message = save_error; }
    }

    fn preview_pauses(&mut self, ctx: &egui::Context, gaps: bool) {
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let rate = self.player.sample_rate();
        let thresh_db = self.pause_threshold_db;
        let min_sec = f64::from(self.pause_min_seconds);
        let snapshot = self.splitter.snapshot();
        self.start_job(ctx, self.text("Pausen werden gesucht …", "Finding pauses …"), move || {
            let mut proposal = TrackSplitter::new();
            proposal.restore(snapshot);
            if gaps {
                proposal.detect_silence_pauses_as_gaps(&left, &right, rate, thresh_db, min_sec);
            } else {
                proposal.detect_silence_pauses(&left, &right, rate, thresh_db, min_sec);
            }
            Ok(JobResult::Pauses(proposal.snapshot()))
        });
    }

}

impl eframe::App for TapeDigizerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_job();
        if let Some(error) = self.player.take_stream_error() {
            self.ui_error_message = Some(format!("{}: {error}", self.text("Wiedergabegerät", "Playback device")));
        }
        #[cfg(target_os = "macos")]
        self.handle_macos_menu(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.can_leave_project() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.recorder.is_busy() || self.recorder.state() == RecorderState::Monitoring || self.player.is_playing() || self.song_editor.is_playing() {
            ctx.request_repaint();
        }

        self.recorder.poll_auto_stop();
        if self.recorder.poll_finished() {
            self.finish_recording_and_analyze(ctx);
        }

        // Global Command Shortcuts - read all keys in a single lock-free query
        let (cmd, shift, key_n, key_s, key_o, key_e, key_f, key_t, key_z, key_y, key_space, key_m, key_h, key_l, key_c, key_v, key_r, key_esc, key_del, key_left, key_right, key_1, key_3) = ctx.input(|i| (
            i.modifiers.command,
            i.modifiers.shift,
            i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::O),
            i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::F),
            i.key_pressed(egui::Key::T),
            i.key_pressed(egui::Key::Z),
            i.key_pressed(egui::Key::Y),
            i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::M),
            i.key_pressed(egui::Key::H),
            i.key_pressed(egui::Key::L),
            i.key_pressed(egui::Key::C),
            i.key_pressed(egui::Key::V),
            i.key_pressed(egui::Key::R),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Num1),
            i.key_pressed(egui::Key::Num3),
        ));

        if key_esc {
            if let Some(before) = self.marker_drag.take() {
                self.splitter.restore(before);
                self.marker_drag_cancelled = true;
            }
            self.scope_locked_center = None;
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            if let Some(before) = self.marker_drag.take() {
                self.splitter.commit_edit(before);
            }
            self.marker_drag_cancelled = false;
            self.scope_locked_center = None;
        }

        if cmd && key_1 {
            self.console_tab = ConsoleTab::Input;
        }
        if cmd && key_3 {
            self.console_tab = ConsoleTab::Edit;
        }
        if cmd && shift && key_e {
            self.export_song = None;
            self.console_tab = ConsoleTab::Export;
        }
        if cmd && key_f {
            self.console_tab = ConsoleTab::Filter;
        }
        if cmd && key_t {
            self.console_tab = ConsoleTab::Tracks;
        }
        if cmd && key_l {
            self.console_tab = ConsoleTab::Edit;
        }

        if cmd && key_n {
            self.new_project();
        }
        if cmd && key_s {
            if shift {
                self.save_project_dialog(ctx);
            } else {
                self.save_project_quick(ctx);
            }
        }
        if cmd && key_o {
            self.open_project_dialog(ctx);
        }

        if !ctx.wants_keyboard_input() && self.job.is_none() && !self.recorder.is_busy() {
            if cmd && key_z {
                if shift {
                    self.undo_marker_edit(ctx, true);
                } else {
                    self.undo_marker_edit(ctx, false);
                }
            } else if cmd && key_y {
                self.undo_marker_edit(ctx, true);
            }
        }

        let has_audio = self.player.total_seconds() > 0.1;
        if !ctx.wants_keyboard_input() && self.job.is_none() && !self.recorder.is_busy() {
            if self.pending_delete_marker.is_some() && key_esc {
                self.pending_delete_marker = None;
            }

            if !cmd && has_audio && key_space {
                if self.visible_tab() == ConsoleTab::Filter { self.song_editor.toggle_play(); }
                else { self.with_recording_output(AudioPlayer::toggle_play); }
            }
            if !cmd && has_audio && key_m && self.visible_tab() != ConsoleTab::Filter {
                let cur_sec = self.player.current_seconds();
                self.add_marker(cur_sec);
            }
            if !cmd && key_h && self.visible_tab() != ConsoleTab::Filter {
                self.hover_preview_enabled = !self.hover_preview_enabled;
            }
            if !cmd && key_l {
                self.console_tab = ConsoleTab::Edit;
            }
            if !cmd && key_f {
                self.console_tab = ConsoleTab::Filter;
            }
            if self.visible_tab() == ConsoleTab::Filter {
                let step = if shift { 0.5 } else { 0.05 };
                if key_left { self.song_editor.seek_by(-step); }
                if key_right { self.song_editor.seek_by(step); }
                if key_r { self.song_editor.use_range_tool(true); }
                if key_v || key_esc { self.song_editor.use_range_tool(false); }
            }
            if let Some(m_idx) = self.selected_marker_idx.filter(|_| self.visible_tab() != ConsoleTab::Filter) {
                let total_sec = self.player.total_seconds();
                let delta = if shift { 0.5 } else { 0.05 };
                if key_left {
                    self.nudge_marker(m_idx, -delta, total_sec);
                }
                if key_right {
                    self.nudge_marker(m_idx, delta, total_sec);
                }
            }
            if !cmd && (key_s || key_c) {
                self.active_edit_tool = EditTool::Scissor;
            }
            if !cmd && key_r {
                self.active_edit_tool = EditTool::Select;
            }
            if (!cmd && key_v) || key_esc {
                self.active_edit_tool = EditTool::Pointer;
            }
            if self.visible_tab() != ConsoleTab::Filter && key_del && !ctx.input(|input| input.events.iter().any(|event| matches!(
                event, egui::Event::Key { key: egui::Key::Delete | egui::Key::Backspace, repeat: true, .. }
            ))) {
                if let Some(index) = self.selected_marker_idx {
                    self.request_marker_delete(index);
                }
            }
        }

        self.render_console_header(ctx);
        self.render_console(ctx);
        if let Some(index) = self.pending_delete_marker.take() {
            self.clear_hover_audition();
            self.splitter.remove_marker_at_index(index);
            self.selected_marker_idx = None;
            self.selected_segment_idx = None;
            ctx.request_repaint();
        }
    }
}

fn timecode(seconds: f64) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}.{:03}", millis / 60_000, (millis / 1000) % 60, millis % 1000)
}

fn parse_timecode(value: &str) -> Option<f64> {
    let value = value.replace(',', ".");
    if let Some((minutes, seconds)) = value.split_once(':') {
        let seconds = seconds.parse::<f64>().ok()?;
        (0.0..60.0).contains(&seconds).then_some(minutes.parse::<u64>().ok()? as f64 * 60.0 + seconds)
    } else {
        value.parse::<f64>().ok().filter(|seconds| seconds.is_finite())
    }
}

impl TapeDigizerApp {
    fn visible_tab(&self) -> ConsoleTab {
        if self.recorder.is_busy() {
            ConsoleTab::Input
        } else {
            self.console_tab
        }
    }

    fn render_console_header(&mut self, ctx: &egui::Context) {
        let p = palette(ctx);
        let busy = self.recorder.is_busy() || self.job.is_some();
        let in_editing = self.visible_tab() != ConsoleTab::Input;

        egui::TopBottomPanel::top("console_header")
            .exact_height(44.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(p.bg_toolbar)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0)),
            )
            .show(ctx, |ui| {
                // Natives Fenster-Verschieben über die freie Fläche der macOS Unified Titlebar
                let bar_rect = ui.max_rect().expand2(Vec2::new(12.0, 8.0));
                let drag_resp = ui.interact(bar_rect, ui.id().with("titlebar_drag"), egui::Sense::click_and_drag());
                if drag_resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                ui.horizontal(|ui| {
                    // Platz für native macOS Traffic Lights in Fullsize Content View
                    #[cfg(target_os = "macos")]
                    ui.add_space(68.0);

                    studio_wordmark(ui);
                    ui.add_space(10.0);

                    let mut is_editing = in_editing;
                    let can_switch = !self.recorder.is_busy();
                    ui.add_enabled_ui(can_switch, |ui| {
                        if segmented_control(
                            ui,
                            "main_workspace_segment",
                            &mut is_editing,
                            &[
                                (false, self.text("Aufnahme", "Recording"), "⌘1"),
                                (true, self.text("Schnitt", "Editing"), "⌘3"),
                            ],
                            HEIGHT_PRIMARY,
                        ) {
                            if !is_editing && self.console_tab == ConsoleTab::Filter {
                                self.song_editor.suspend();
                            }
                            self.console_tab = if is_editing {
                                ConsoleTab::Edit
                            } else {
                                ConsoleTab::Input
                            };
                        }
                    });

                    // Im Schnitt-Modus direkt die vollständige Workflow-Navigation inkl. Einzeltrack
                    if in_editing {
                        ui.add_space(8.0);
                        let mut current = self.visible_tab();
                        ui.add_enabled_ui(!busy, |ui| {
                            if segmented_control(
                                ui,
                                "cut_subview_segment",
                                &mut current,
                                &[
                                    (ConsoleTab::Edit, self.text("Marker", "Cut"), "⌘3"),
                                    (ConsoleTab::Tracks, self.text("Tracks", "Tracks"), "⌘T"),
                                    (ConsoleTab::Filter, self.text("Einzeltrack", "Track FX"), "⌘F"),
                                    (ConsoleTab::Export, self.text("Export", "Export"), "⇧⌘E"),
                                ],
                                HEIGHT_PRIMARY,
                            ) {
                                if self.console_tab == ConsoleTab::Filter && current != ConsoleTab::Filter {
                                    self.song_editor.suspend();
                                }
                                if current == ConsoleTab::Export && self.console_tab != ConsoleTab::Export {
                                    self.export_song = None;
                                }
                                self.console_tab = current;
                            }
                        });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if bevel_button_sized(ui, self.language.code(), 36.0, BevelStyle::Neutral)
                            .on_hover_text(self.language.display_name())
                            .clicked()
                        {
                            self.language = if self.language == Language::De { Language::En } else { Language::De };
                            self.save_config();
                        }
                        let mode = theme_mode(ctx);
                        let theme_label = match mode {
                            ThemeMode::Silverface => self.text("Dunkel", "Dark"),
                            ThemeMode::Obsidian => self.text("Hell", "Light"),
                        };
                        if bevel_button_sized(ui, theme_label, 58.0, BevelStyle::Neutral)
                            .on_hover_text(self.text(
                                "Zwischen Silverface (Hell) und Obsidian (Dunkel) wechseln",
                                "Switch between Silverface (Light) and Obsidian (Dark)",
                            ))
                            .clicked()
                        {
                            let next_mode = match mode {
                                ThemeMode::Silverface => ThemeMode::Obsidian,
                                ThemeMode::Obsidian => ThemeMode::Silverface,
                            };
                            apply_theme(ctx, next_mode);
                        }
                    });
                });

                // Subtile Trennlinie unter der Unified Titlebar
                ui.painter().line_segment(
                    [Pos2::new(bar_rect.min.x, bar_rect.max.y), Pos2::new(bar_rect.max.x, bar_rect.max.y)],
                    Stroke::new(1.0, p.border_subtle),
                );
            });
    }

    fn render_record_control(&mut self, ui: &mut Ui) {
        let p = palette(ui.ctx());
        let state = self.recorder.state();
        let label = match state {
            RecorderState::WaitingForSignal => self.text("Warten abbrechen", "Cancel waiting").to_string(),
            RecorderState::Recording => {
                if self.run_on_seconds > 0 {
                    format!("{} · {} s", self.text("Stopp mit Nachlauf", "Stop with tail"), self.run_on_seconds)
                } else {
                    self.text("Aufnahme stoppen", "Stop recording").to_string()
                }
            }
            RecorderState::RunOn => self.text("Sofort stoppen", "Stop immediately").to_string(),
            RecorderState::Finalizing => self.text("WAV abschließen …", "Finalizing WAV …").to_string(),
            _ => self.text("● Aufnahme starten", "● Record").to_string(),
        };
        let ready = self.job.is_none() && (state != RecorderState::Finalizing)
            && (self.recorder.is_busy() || self.current_channel_pair().is_some());
        if ui.add_enabled_ui(ready, |ui| bevel_button_sized(ui, &label, 148.0, BevelStyle::Record)).inner.clicked() {
            if state == RecorderState::RunOn {
                self.recorder.stop_immediately();
            } else {
                self.record_or_stop();
            }
        }
        if state == RecorderState::Recording && self.run_on_seconds > 0
            && bevel_button(ui, self.text("Sofort stoppen", "Stop immediately"), BevelStyle::Neutral).clicked()
        {
            self.recorder.stop_immediately();
        }

        match state {
            RecorderState::WaitingForSignal => {
                ui.spinner();
                ui.label(self.text("Warte auf Signal", "Waiting for signal"));
            }
            RecorderState::Recording | RecorderState::RunOn => {
                let elapsed = timecode(self.recorder.elapsed_duration().as_secs_f64());
                if state == RecorderState::RunOn {
                    let remaining = self.recorder.remaining_duration().map_or(0.0, |time| time.as_secs_f64());
                    ui.colored_label(p.amber, RichText::new(format!("{} {:.0} s", self.text("Nachlauf", "Tail"), remaining.ceil())).monospace());
                } else {
                    time_display(ui, format!("REC {elapsed}"));
                }
            }
            RecorderState::Monitoring => {
                ui.colored_label(p.teal, self.text("● Pegelcheck aktiv", "● Monitoring active"));
            }
            _ => {}
        }
    }

    fn render_console(&mut self, ctx: &egui::Context) {
        let has_audio = self.player.total_seconds() > 0.1 && !self.waveform_cache.is_empty();
        let editable = !self.recorder.is_busy() && self.job.is_none() && has_audio;
        let total = self.player.total_seconds();
        let cursor = self.player.current_seconds();
        if self.visible_tab() == ConsoleTab::Filter {
            self.player.pause();
            self.player.stop_audition();
        } else {
            self.song_editor.finish_edit(&mut self.splitter);
            self.song_editor.suspend();
        }

        let p = palette(ctx);
        let show_top_bridge = self.visible_tab() == ConsoleTab::Input
            || (self.visible_tab() != ConsoleTab::Filter && has_audio)
            || self.ui_error_message.is_some()
            || self.job.as_ref().is_some_and(|job| job.export_progress.is_none());

        if show_top_bridge {
            egui::TopBottomPanel::top("console_meter_bridge")
                .show_separator_line(false)
                .frame(egui::Frame::none().fill(p.bg_window).inner_margin(egui::Margin::symmetric(10.0, 4.0)))
                .show(ctx, |ui| {
                    if self.visible_tab() == ConsoleTab::Input {
                        surface_bay(ui, |ui| {
                            ui.horizontal_wrapped(|ui| self.render_record_control(ui));
                            ui.add_space(6.0);
                            rack_columns(ui, 2, |cols| {
                                self.render_input_meter(&mut cols[0]);
                                self.render_recording_report(&mut cols[1]);
                            });
                        });
                    } else if self.visible_tab() != ConsoleTab::Filter {
                        surface_bay(ui, |ui| {
                            ui.add_enabled_ui(self.job.is_none(), |ui| self.render_transport(ui));
                        });
                    }
                    if let Some(error) = &self.ui_error_message {
                        let dismiss = self.text("Schließen", "Dismiss");
                        let dismissed = ui.horizontal(|ui| {
                            let dismissed = bevel_button(ui, dismiss, BevelStyle::Compact).clicked();
                            ui.add(egui::Label::new(RichText::new(error).color(p.red)).truncate())
                                .on_hover_text(error);
                            dismissed
                        }).inner;
                        if dismissed {
                            self.ui_error_message = None;
                        }
                    }
                    if let Some(job) = self.job.as_ref().filter(|job| job.export_progress.is_none()) {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(job.label);
                        });
                    }
                });
        }

        // Schlanke Footer-Statusleiste wie im Silverface-/Obsidian-Entwurf
        egui::TopBottomPanel::bottom("console_status_footer")
            .exact_height(24.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(p.bg_toolbar)
                    .inner_margin(egui::Margin::symmetric(12.0, 3.0)),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect().expand2(Vec2::new(12.0, 3.0));
                ui.painter().line_segment(
                    [Pos2::new(rect.min.x, rect.min.y), Pos2::new(rect.max.x, rect.min.y)],
                    Stroke::new(1.0, p.border_subtle),
                );
                ui.horizontal(|ui| {
                    let changed = (self.current_audio_path.is_some() || self.current_project_path.is_some())
                        && self.has_unsaved_edits();
                    let project_lbl = format!(
                        "● {} · {}{}",
                        self.tape_name,
                        self.tape_side.label(self.language),
                        if changed { " •" } else { "" }
                    );
                    ui.label(RichText::new(project_lbl).size(10.5).color(p.text_secondary));
                    ui.label(RichText::new("|").size(10.5).color(p.border_mid));
                    ui.label(RichText::new(self.media_source.label(self.language)).size(10.5).color(p.text_muted));
                    if let Some(dev) = self.current_device() {
                        ui.label(RichText::new("|").size(10.5).color(p.border_mid));
                        ui.add(
                            egui::Label::new(RichText::new(&dev.name).size(10.5).color(p.teal))
                                .truncate(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status_tc = if self.recorder.is_busy() {
                            timecode(self.recorder.elapsed_duration().as_secs_f64())
                        } else {
                            timecode(total)
                        };
                        ui.label(RichText::new(status_tc).monospace().size(10.5).color(p.amber));
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg_window).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .show(ctx, |ui| {
                #[cfg(test)]
                let bounds = ui.max_rect();
                let available = ui.available_height();
                let tab = self.visible_tab();
                if tab != ConsoleTab::Edit || !editable {
                    self.clear_hover_audition();
                }

                match tab {
                    _ if tab != ConsoleTab::Input && !has_audio => self.render_edit_welcome(ui, ctx),
                    ConsoleTab::Input => {
                        let wave_height = (available - 296.0).clamp(70.0, 220.0);
                        self.render_live_timeline(ui, wave_height);
                        ui.add_space(6.0);
                        self.render_input_setup(ui);
                    }
                    ConsoleTab::Edit => {
                        ui.add_enabled_ui(editable, |ui| {
                            let legacy_panel_id = egui::Id::new("whole_recording_processing");
                            let mut whole_recording = ui.data(|data| data.get_temp::<bool>(legacy_panel_id)).unwrap_or(false);
                            let wave_height = if whole_recording {
                                (ui.available_height() * 0.22).clamp(84.0, 210.0)
                            } else {
                                (ui.available_height() * 0.27).clamp(108.0, 260.0)
                            };
                            self.render_timeline(ui, total, cursor, true, wave_height);
                            ui.add_space(8.0);
                            surface_bay(ui, |ui| {
                                ui.horizontal(|ui| {
                                    segmented_control(
                                        ui,
                                        "edit_detail_mode",
                                        &mut whole_recording,
                                        &[
                                            (false, self.text("Schnitt-Lupe", "Cut magnifier"), ""),
                                            (true, self.text("Aufnahme bearbeiten", "Process recording"), ""),
                                        ],
                                        HEIGHT_CONTROL,
                                    );
                                });
                                ui.add_space(6.0);
                                ui.data_mut(|data| data.insert_temp(legacy_panel_id, whole_recording));
                                if whole_recording {
                                    self.render_denoiser_and_cut_panel(ui, total, cursor);
                                } else {
                                    self.render_detail_panel(ui, total, cursor);
                                }
                            });
                        });
                    }
                    ConsoleTab::Filter => {
                        let tracks = self.splitter.get_tracks(total);
                        let legacy = self.legacy_song_settings();
                        let action = ui.add_enabled_ui(editable, |ui| {
                            self.song_editor.show(ui, self.player.get_samples_l(), self.player.get_samples_r(), self.player.sample_rate(),
                                &tracks, &mut self.selected_segment_idx, &mut self.splitter, &legacy, self.export_highpass_20hz, &self.saved_noise_presets, self.language)
                        }).inner;
                        if let Some(action) = action {
                            match action {
                                SongEditorAction::BackToCut => {
                                    self.song_editor.suspend();
                                    self.console_tab = ConsoleTab::Edit;
                                    ctx.request_repaint();
                                }
                                SongEditorAction::Export(segment) => {
                                    self.song_editor.suspend();
                                    self.export_outcome = None;
                                    self.export_song = Some(segment);
                                    self.export_page = 0;
                                    self.console_tab = ConsoleTab::Export;
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                    ConsoleTab::Tracks => {
                        let tracks = self.splitter.get_tracks(total);
                        ui.add_enabled_ui(editable, |ui| {
                            surface_bay(ui, |ui| self.render_track_list(ui, &tracks));
                        });
                    }
                    ConsoleTab::Export => {
                        let exporting = self.job.as_ref().is_some_and(|job| job.export_progress.is_some());
                        ui.add_enabled_ui(exporting || self.job.is_none(), |ui| {
                            surface_bay(ui, |ui| {
                                self.render_export_panel(ui);
                            });
                        });
                    }
                }
                #[cfg(test)]
                assert!(
                    bounds.expand(1.0).contains_rect(ui.min_rect()),
                    "Console tab {} (lang={:?}, mod={}) overflows: {:?} vs {:?}", tab as u8, self.language, self.song_editor.layout_test_module(), ui.min_rect(), bounds
                );
            });
    }

    fn render_edit_welcome(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let p = palette(ctx);
        ui.add_space(20.0);
        surface_bay(ui, |ui| {
            bay_header(
                ui,
                self.text("Schnitt & Track-Bearbeitung", "Editing & track processing"),
                self.text("Noch kein Projekt oder Audio geladen", "No project or audio loaded yet"),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(self.text("Aus Aufnahmen werden fertige Tracks", "Turn recordings into finished tracks"))
                    .size(16.0)
                    .strong()
                    .color(p.text_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(self.text(
                    "Lade ein bestehendes TapeDigizer-Projekt oder eine WAV-Datei, um Schnittmarken zu setzen, einzelne Tracks klanglich aufzubereiten und zu exportieren.",
                    "Load an existing TapeDigizer project or a WAV file to set cut markers, process individual tracks, and export.",
                ))
                .color(p.text_secondary),
            );
            ui.add_space(14.0);
            ui.add_enabled_ui(self.job.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if bevel_button_sized(
                        ui,
                        self.text("Projekt / WAV laden …", "Load project / WAV …"),
                        186.0,
                        BevelStyle::Primary,
                    )
                    .on_hover_text("⌘O")
                    .clicked()
                    {
                        self.open_project_dialog(ctx);
                    }
                    ui.add_space(4.0);
                    if bevel_button_sized(
                        ui,
                        self.text("Neue Aufnahme vorbereiten", "Prepare a new recording"),
                        186.0,
                        BevelStyle::Neutral,
                    )
                    .on_hover_text("⌘1")
                    .clicked()
                    {
                        self.console_tab = ConsoleTab::Input;
                    }
                });
            });
            ui.add_space(18.0);
            panel_row(ui, 3, PanelStyle::Inset, |cols| {
                for (col, title, description) in [
                    (0, self.text("1. Schnitt & Marker", "1. Cut & markers"), self.text("Schnittmarken, Stille-Erkennung und Trackgrenzen präzise festlegen.", "Set cut markers, silence detection and track boundaries.")),
                    (1, self.text("2. Track-Bearbeitung", "2. Process track"), self.text("Einzelne Tracks fokussiert mit Rauschfilter, EQ, Dynamik & Routing aufbereiten.", "Process individual tracks with denoiser, EQ, dynamics & routing.")),
                    (2, self.text("3. Export", "3. Export"), self.text("Tracks benennen, normalisieren und sauber als WAV-Dateien ausgeben.", "Name tracks, normalize and export clean WAV files.")),
                ] {
                    let ui = &mut cols[col];
                    ui.label(RichText::new(title).strong().color(p.text_primary));
                    ui.add_space(3.0);
                    ui.label(RichText::new(description).size(11.5).color(p.text_secondary));
                }
            });
        });
    }

    fn render_live_timeline(&mut self, ui: &mut Ui, waveform_height: f32) {
        let p = palette(ui.ctx());
        surface_bay(ui, |ui| {
            ui.horizontal(|ui| {
                panel_title(ui, self.text("Band-Transport & Live-Wellenform", "Tape transport & live waveform"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("PCM 24-bit · WAV · Stereo");
                });
            });
            ui.add_space(6.0);
            let state = self.recorder.state();
            let recording = matches!(state, RecorderState::Recording | RecorderState::RunOn);
            let running = recording || state == RecorderState::Monitoring;
            let elapsed = self.recorder.elapsed_duration().as_secs_f64();
            let limit = (self.selected_timer_minutes > 0)
                .then(|| f64::from(self.selected_timer_minutes + self.time_headroom_minutes) * 60.0 + f64::from(self.run_on_seconds));
            let status = match state {
                RecorderState::WaitingForSignal => self.text("Warte auf Eingangssignal …", "Waiting for input signal …"),
                RecorderState::Monitoring => self.language.live_waveform_monitoring(),
                RecorderState::Finalizing => self.text("WAV wird abgeschlossen …", "Finalizing WAV …"),
                _ => self.language.live_waveform_idle(),
            };
            let nominal_secs = f64::from(self.selected_timer_minutes.max(30)) * 60.0;
            let deck_h = waveform_height.clamp(70.0, 180.0);
            rack_columns(ui, 2, |columns| {
                draw_tape_transport_deck(
                    &mut columns[0],
                    deck_h,
                    elapsed,
                    nominal_secs,
                    running,
                    recording,
                    self.media_source.label(self.language),
                );
                self.recorder.with_waveform_data(|points| {
                    let points = if recording { points } else { &[] };
                    crate::ui::waveform_widget::render_live_recording_waveform(
                        &mut columns[1], points, elapsed, limit, recording, status, deck_h,
                    );
                });
            });
            if recording {
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    if let Some(remaining) = self.recorder.remaining_duration() {
                        ui.monospace(format!("{} {}", self.text("Verbleibend", "Remaining"), timecode(remaining.as_secs_f64())));
                    }
                    if self.enable_silence_autostop {
                        let silence = self.recorder.silence_seconds_detected();
                        if silence > 0.5 {
                            ui.colored_label(p.amber, format!("{} {silence:.1} / {:.0} s", self.text("Stille", "Silence"), self.autostop_silence_secs));
                        }
                    }
                });
            }
        });
    }

    fn record_or_stop(&mut self) {
        if self.recorder.is_busy() {
            self.recorder.request_stop();
            return;
        }
        let (Some(device), Some(pair)) = (self.current_device(), self.current_channel_pair()) else {
            return;
        };
        let device_name = device.name.clone();
        if !self.can_leave_project() {
            return;
        }
        self.player.pause();
        self.console_tab = ConsoleTab::Input;
        self.recorder.configure_autostop(
            self.autostop_threshold_db,
            self.autostop_silence_secs,
            self.run_on_seconds,
        );
        let total_minutes = if self.selected_timer_minutes != 0 {
            self.selected_timer_minutes + self.time_headroom_minutes
        } else {
            0
        };
        let timer = (total_minutes != 0)
            .then(|| Duration::from_secs(u64::from(total_minutes) * 60));
        self.ui_error_message = self.next_recording_path().and_then(|target| {
            self.recorder.start_recording(&device_name, pair, target, timer, self.enable_silence_autostop)
        }).err();
    }

    fn render_transport(&mut self, ui: &mut Ui) {
        let total = self.player.total_seconds();
        let position = self.player.current_seconds();
        ui.horizontal_wrapped(|ui| {
            let play = if self.player.is_playing() { self.text("Pause", "Pause") } else { self.text("Wiedergabe", "Play") };
            if ui.add_enabled_ui(total > 0.0, |ui| {
                bevel_button_sized(
                    ui,
                    play,
                    92.0,
                    if self.player.is_playing() { BevelStyle::Primary } else { BevelStyle::Neutral },
                )
            }).inner.on_hover_text(self.text("Leertaste", "Space")).clicked() {
                self.with_recording_output(AudioPlayer::toggle_play);
            }
            if ui.add_enabled_ui(total > 0.0, |ui| {
                bevel_button_sized(ui, self.language.btn_stop(), 68.0, BevelStyle::Neutral)
            }).inner.clicked() {
                self.player.stop();
            }
            time_display(ui, format!("{}  /  {}", timecode(position), timecode(total)));
            ui.add_space(6.0);
            if ui.add_enabled_ui(total > 0.0, |ui| {
                bevel_button(ui, self.text("Marker setzen", "Add marker"), BevelStyle::Neutral)
            }).inner.on_hover_text("M").clicked()
            {
                self.add_marker(position);
            }
        });
    }

    fn add_marker(&mut self, position: f64) {
        let snapped = self.snap_time_if_enabled(position);
        if let Some(index) = self.splitter.add_marker(snapped, self.player.total_seconds(), self.player.sample_rate()) {
            self.select_marker(index);
        }
    }

    fn select_marker(&mut self, index: usize) {
        self.selected_marker_idx = Some(index);
        self.selected_segment_idx = Some(index + 1);
        self.selected_region = None;
        self.scope_locked_center = None;
    }

    fn set_scope_region(&mut self, region: (f64, f64)) {
        self.selected_region = Some(region);
        self.scope_zoom = 1.0;
        self.scope_scroll = 0.0;
    }

    fn request_marker_delete(&mut self, index: usize) {
        if index < self.splitter.markers.len() {
            self.pending_delete_marker = Some(index);
        }
    }

    fn render_input_setup(&mut self, ui: &mut Ui) {
        let p = palette(ui.ctx());
        ui.add_enabled_ui(!self.recorder.is_busy() && self.job.is_none(), |ui| {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                panel_row(ui, 2, PanelStyle::Surface, |columns| {
                    let ui = &mut columns[0];
                    panel_title(ui, self.text("Quelle & Projekt", "Source & project"));
                    ui.add_space(3.0);
                    // Media Source Presets als einheitliches, symmetrisches Segmented Control
                    let mut source = self.media_source;
                    if segmented_control(
                        ui,
                        "media_source_segment",
                        &mut source,
                        &[
                            (MediaSourceType::Cassette, MediaSourceType::Cassette.label(self.language), ""),
                            (MediaSourceType::ReelToReel, MediaSourceType::ReelToReel.label(self.language), ""),
                            (MediaSourceType::VinylLp, MediaSourceType::VinylLp.label(self.language), ""),
                            (MediaSourceType::VinylSingle, MediaSourceType::VinylSingle.label(self.language), ""),
                        ],
                        HEIGHT_CONTROL,
                    ) {
                        self.apply_media_source_preset(source);
                    }
                    ui.add_space(4.0);

                    // Album/Bandname + Interpret/Artist side-by-side
                    ui.columns(2, |cols| {
                        cols[0].label(RichText::new(self.text("Medium / Album", "Tape / album")).color(p.text_secondary).small());
                        let w0 = cols[0].available_width();
                        let name_resp = cols[0].add(
                            egui::TextEdit::singleline(&mut self.tape_name)
                                .desired_width(w0)
                                .hint_text(self.language.hint_tape_name()),
                        );
                        if name_resp.lost_focus() {
                            self.save_config();
                        }

                        cols[1].label(RichText::new(self.text("Interpret (optional)", "Artist (optional)")).color(p.text_secondary).small());
                        let w1 = cols[1].available_width();
                        let artist_hint = self.text("z. B. Pink Floyd", "e.g. Pink Floyd");
                        let artist_resp = cols[1].add(
                            egui::TextEdit::singleline(&mut self.artist_name)
                                .desired_width(w1)
                                .hint_text(artist_hint),
                        );
                        if artist_resp.lost_focus() {
                            self.save_config();
                        }
                    });

                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        segmented_control(
                            ui,
                            "tape_side_segment",
                            &mut self.tape_side,
                            &[
                                (TapeSide::SideA, TapeSide::SideA.label(self.language), ""),
                                (TapeSide::SideB, TapeSide::SideB.label(self.language), ""),
                            ],
                            HEIGHT_CONTROL,
                        );
                        if bevel_button_sized(ui, self.language.btn_change_folder(), 104.0, BevelStyle::Neutral).clicked() {
                            if let Some(folder) = rfd::FileDialog::new().set_directory(&self.export_directory).pick_folder() {
                                self.export_directory = folder;
                                self.save_config();
                            }
                        }
                        if bevel_button_sized(ui, self.text("Im Finder", "Reveal"), 84.0, BevelStyle::Neutral).clicked() {
                            self.reveal_session_in_finder();
                        }
                    });
                    let folder = self.session_project_dir().display().to_string();
                    ui.add(egui::Label::new(RichText::new(&folder).color(p.text_muted).small()).truncate())
                        .on_hover_text(folder);
                    ui.add_space(4.0);
                    panel_title(ui, self.text("Audioeingang", "Audio input"));
                    self.render_input_routing(ui);


                    let ui = &mut columns[1];
                    panel_title(ui, self.text("Aufnahmedauer & Auto-Stopp", "Duration & auto-stop"));
                    ui.add_space(3.0);
                    let before_timer = self.selected_timer_minutes;
                    let before_headroom = self.time_headroom_minutes;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        ui.label(RichText::new(self.text("Dauer:", "Length:")).color(p.text_secondary).small());
                        for (minutes, label, w) in [
                            (10, "10m", 44.0), (25, "25m", 44.0), (30, "30m", 44.0),
                            (45, "45m", 44.0), (60, "60m", 44.0), (90, "90m", 44.0),
                            (0, self.language.preset_unlimited(), 78.0),
                        ] {
                            if bevel_button_sized(
                                ui,
                                label,
                                w,
                                if self.selected_timer_minutes == minutes { BevelStyle::Engaged } else { BevelStyle::Neutral },
                            ).clicked() {
                                self.selected_timer_minutes = minutes;
                            }
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(self.text("Zeit-Headroom:", "Time Headroom:")).color(p.text_secondary).small());
                        for (mins, label) in [(0, "+0m"), (2, "+2m"), (5, "+5m"), (10, "+10m"), (15, "+15m")] {
                            if ui.selectable_label(self.time_headroom_minutes == mins, label)
                                .on_hover_text(self.text(
                                    "Zusätzlicher Zeit-Puffer über die Nenn-Bandlänge hinaus (schützt vor Abschneiden bei überlangen Kassetten)",
                                    "Safety time buffer added to nominal tape length (prevents cutoff on tapes longer than nominal length)",
                                ))
                                .clicked()
                            {
                                self.time_headroom_minutes = mins;
                            }
                        }
                        ui.spacing_mut().slider_width = 55.0;
                        ui.add(egui::Slider::new(&mut self.time_headroom_minutes, 0..=30).suffix(" m"));
                    });

                    let mut cfg_changed = self.selected_timer_minutes != before_timer
                        || self.time_headroom_minutes != before_headroom;

                    ui.horizontal_wrapped(|ui| {
                        let auto_stop = self.text("Auto-Stopp bei Stille", "Auto-stop on silence");
                        if ui.checkbox(&mut self.enable_silence_autostop, auto_stop)
                            .on_hover_text(self.language.checkbox_silence_autostop()).changed()
                        {
                            cfg_changed = true;
                        }
                        ui.separator();
                        ui.label(RichText::new(self.text("Nachlauf:", "Tail:")).color(p.text_secondary).small());
                        for secs in [0_u32, 10, 15, 30, 60] {
                            let lbl = format!("{secs}s");
                            if ui.selectable_label(self.run_on_seconds == secs, lbl)
                                .on_hover_text(self.text("Nachlaufzeit beim manuellen Stopp", "Tail duration when stopping manually"))
                                .clicked()
                            {
                                self.run_on_seconds = secs;
                                cfg_changed = true;
                            }
                        }
                    });

                    if self.enable_silence_autostop {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(self.text("Schwelle:", "Threshold:")).color(p.text_secondary).small());
                            for (db, label) in [
                                (-100.0, self.text("-100 dB (Kein Signal)", "-100 dB (No signal)")),
                                (-90.0, "-90 dB"),
                                (-80.0, "-80 dB"),
                                (-65.0, "-65 dB"),
                            ] {
                                if ui.selectable_label((self.autostop_threshold_db - db).abs() < 1.0, label)
                                    .on_hover_text(self.text(
                                        "-100 dBFS erkennt nur absoluten Signalverlust (Deck stoppt/aus), schützt leise Musik",
                                        "-100 dBFS detects only complete signal loss (deck stopped/off), protects quiet music",
                                    ))
                                    .clicked()
                                {
                                    self.autostop_threshold_db = db;
                                    cfg_changed = true;
                                }
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().slider_width = 85.0;
                            let thresh_lbl = self.text("Schwelle", "Threshold");
                            if ui.add(egui::Slider::new(&mut self.autostop_threshold_db, -100.0..=-30.0)
                                .step_by(1.0).text(thresh_lbl).suffix(" dBFS")).changed()
                            {
                                cfg_changed = true;
                            }
                            let dur_lbl = self.text("Haltedauer", "Hold");
                            if ui.add(egui::Slider::new(&mut self.autostop_silence_secs, 3.0..=60.0)
                                .step_by(1.0).text(dur_lbl).suffix(" s")).changed()
                            {
                                cfg_changed = true;
                            }
                        });

                    }

                    if cfg_changed {
                        self.recorder.configure_autostop(
                            self.autostop_threshold_db,
                            self.autostop_silence_secs,
                            self.run_on_seconds,
                        );
                        self.save_config();
                    }
                });
            });
        });
    }

    fn render_input_routing(&mut self, ui: &mut Ui) {
        ui.add_enabled_ui(!self.recorder.is_busy() && self.job.is_none(), |ui| {
            ui.horizontal(|ui| {
                let device = self.current_device().map(|device| format!("{}  ·  {} ch", device.name, device.channels))
                    .unwrap_or_else(|| self.language.label_no_device().into());
                let selected = self.selected_device_idx;
                let combo_w = (ui.available_width() - 126.0).max(120.0);
                egui::ComboBox::from_id_salt("device_combo")
                    .width(combo_w).truncate().selected_text(device)
                    .show_ui(ui, |ui| {
                        for (index, device) in self.available_devices.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_device_idx, index, &device.name);
                        }
                    });
                if self.selected_device_idx != selected {
                    self.recorder.stop_monitoring();
                    if let Some(device) = self.current_device() {
                        self.channel_pairs = get_channel_pairs_for_device(device);
                        self.selected_pair_idx = 0;
                        self.save_config();
                    }
                }
                if ui.add(action_button(format!("↻ {}", self.language.btn_rescan()))).clicked() {
                    self.recorder.stop_monitoring();
                    self.refresh_device_list();
                }
            });

            let channels = self.current_channel_pair().map(|pair| pair.label)
                .unwrap_or_else(|| self.language.label_select_channel().into());
            let selected = self.selected_pair_idx;
            egui::ComboBox::from_id_salt("channel_combo")
                .width((ui.available_width() - 16.0).max(80.0)).truncate().selected_text(channels)
                .show_ui(ui, |ui| {
                    for (index, pair) in self.channel_pairs.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_pair_idx, index, &pair.label);
                    }
                });
            if self.selected_pair_idx != selected {
                self.recorder.stop_monitoring();
                self.save_config();
            }
        });
    }

    fn render_input_meter(&mut self, ui: &mut Ui) {
        let p = palette(ui.ctx());
        let monitoring = self.recorder.state() == RecorderState::Monitoring;
        let active = monitoring || self.recorder.is_busy();
        let meter = if active { self.recorder.get_meter() } else { Default::default() };
        let vu_mode_id = egui::Id::new("input_meter_vintage_vu");
        let mut use_vu = ui.data(|d| d.get_temp::<bool>(vu_mode_id)).unwrap_or(true);
        ui.horizontal(|ui| {
            panel_title(ui, self.text("Eingangspegel", "Input levels"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                segmented_control(
                    ui,
                    "input_meter_style",
                    &mut use_vu,
                    &[(true, "VU", ""), (false, "LED", "")],
                    HEIGHT_COMPACT,
                );
            });
        });
        ui.data_mut(|d| d.insert_temp(vu_mode_id, use_vu));
        ui.scope(|ui| {
            let reset_clicked = if use_vu {
                render_vintage_vu_meter(
                    ui,
                    &meter,
                    self.text("Eingangspegel", "Input levels"),
                    self.text("Peak und CLIP zurücksetzen", "Reset peak and clip"),
                    self.text("Übersteuerung erkannt", "Clipping detected"),
                )
            } else {
                render_stereo_meter(
                    ui,
                    &meter,
                    self.text("Eingangspegel", "Input levels"),
                    self.text("Peak und CLIP zurücksetzen", "Reset peak and clip"),
                    self.text("Übersteuerung erkannt", "Clipping detected"),
                )
            };
            if reset_clicked {
                self.recorder.reset_meter_peaks();
            }
            let peak = meter.peak_hold_l_db.max(meter.peak_hold_r_db);
            let (hint, color) = if meter.is_clipping {
                (self.text("Clipping · Gain am Interface senken", "Clipping · Lower interface gain").to_string(), p.red)
            } else if peak > -60.0 {
                let delta = meter.peak_hold_r_db - meter.peak_hold_l_db;
                (format!("Peak {peak:.1} dBFS  ·  R−L {delta:+.1} dB"), p.text_secondary)
            } else if active {
                (self.text("Kein messbarer Eingangsspitzenpegel", "No measurable input peak").to_string(), p.text_secondary)
            } else {
                (self.text("Monitoring aus", "Monitoring off").to_string(), p.text_muted)
            };

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let label = if monitoring {
                    self.text("Pegelcheck beenden", "End level check")
                } else {
                    self.text("Pegel prüfen", "Check levels")
                };
                let ready = !self.recorder.is_busy() && self.job.is_none()
                    && (monitoring || self.current_channel_pair().is_some());
                if ui.add_enabled_ui(ready, |ui| {
                    bevel_button(ui, label, if monitoring { BevelStyle::Engaged } else { BevelStyle::Neutral })
                }).inner.clicked() {
                    if monitoring {
                        self.recorder.stop_monitoring();
                    } else if let (Some(device), Some(pair)) = (self.current_device(), self.current_channel_pair()) {
                        let name = device.name.clone();
                        self.ui_error_message = self.recorder.start_monitoring(&name, pair).err();
                    }
                }
                if ui.add_enabled(active, action_button("Peak/Clip Reset").small()).clicked() {
                    self.recorder.reset_meter_peaks();
                }
                ui.colored_label(color, RichText::new(hint).small().monospace());
            });

        });
    }

    fn render_recording_report(&mut self, ui: &mut Ui) {
        let p = palette(ui.ctx());
        panel_title(ui, self.text("Aufnahmebefund", "Recording analysis"));
        ui.scope(|ui| {
            let report = self.analysis_report.as_ref();
            let values = match report {
                Some(report) => [
                    format!("{:.1}", report.peak_left_db),
                    format!("{:.1}", report.peak_right_db),
                    format!("{:.1}", (-report.max_peak_db).max(0.0)),
                    report.clipped_samples.to_string(),
                ],
                None => ["–".into(), "–".into(), "–".into(), "–".into()],
            };
            let clip_color = if report.is_some_and(|report| report.clipped_samples > 0) {
                p.red
            } else {
                p.text_secondary
            };
            ui.columns(4, |cols| {
                for ((column, (label, color)), value) in cols.iter_mut().zip([
                    ("L · dBFS", p.wave_left),
                    ("R · dBFS", p.wave_right),
                    ("HEADROOM · dB", p.amber),
                    ("CLIP", clip_color),
                ]).zip(values) {
                    meter_readout(column, label, value, color);
                }
            });
            let Some(report) = report else {
                ui.weak(self.text("Verfügbar nach Aufnahme oder WAV-Import.", "Available after recording or importing WAV."));
                return;
            };

            let (gain, color) = match report.diagnosis {
                LevelDiagnosis::Clipped { recommended_cut_db, .. } => (
                    format!("{} −{recommended_cut_db:.1} dB", self.text("Interface-Gain", "Interface gain")), p.red,
                ),
                LevelDiagnosis::TooQuiet { recommended_boost_db, .. } => (
                    format!("{} +{recommended_boost_db:.1} dB", self.text("Interface-Gain", "Interface gain")), p.amber,
                ),
                LevelDiagnosis::Optimal { .. } => (
                    self.text("Pegel im Zielbereich", "Level in target range").into(), p.teal,
                ),
            };
            let balance = report.stereo_balance_db;
            let dc_offset = report.dc_offset_percent;
            let rms = format!("RMS L {:.1} / R {:.1} dBFS  ·  Crest {:.1} dB", report.rms_left_db, report.rms_right_db, report.crest_factor_db);
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, RichText::new(gain).small().strong());
                ui.label(RichText::new(format!("R−L {balance:+.1} dB · DC {dc_offset:.2}%")).monospace().small())
                    .on_hover_text(rms);
                let correction = balance.clamp(-12.0, 12.0);
                let applied = (self.export_balance_db - correction).abs() < 0.05;
                if balance.abs() >= 0.5 && ui.add_enabled(
                    self.job.is_none() && !self.recorder.is_busy(),
                    mode_button(ui, self.text("Export ausgleichen", "Balance export"), applied),
                ).clicked() {
                    self.export_balance_db = if applied { 0.0 } else { correction };
                }
            });
        });
    }

    fn render_timeline(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64, has_audio: bool, waveform_height: f32) {
        let p = palette(ui.ctx());
        ui.scope(|ui| {
            panel_title(ui, self.text("Stereo-Wellenform", "Stereo waveform"));
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tool, name, key) in [
                    (EditTool::Pointer, self.text("Zeiger", "Pointer"), "V"),
                    (EditTool::Scissor, self.text("Schnitt", "Split"), "S"),
                    (EditTool::Select, self.text("Bereich", "Range"), "R"),
                ] {
                    if ui.add(mode_button(ui, format!("{name} · {key}"), self.active_edit_tool == tool)).clicked() {
                        self.active_edit_tool = tool;
                    }
                }
                ui.separator();
                let preview = self.text("Vorhören", "Audition");
                let zc_lbl = self.text("0-Snap", "0-Snap");
                let zc_tip = self.text("Schnittmarken automatisch auf Nulldurchgang einrasten (verhindert Knackser)", "Snap markers to zero crossings to prevent clicks");
                ui.toggle_value(&mut self.hover_preview_enabled, format!("{preview} · H"));
                if ui.checkbox(&mut self.zero_crossing_snap, zc_lbl)
                    .on_hover_text(zc_tip)
                    .changed()
                {
                    self.save_config();
                }
                ui.separator();
                if ui.add_enabled(self.splitter.can_undo(), action_button(self.text("Rückgängig", "Undo")).small())
                    .on_hover_text(self.language.btn_undo()).clicked()
                {
                    self.undo_marker_edit(ui.ctx(), false);
                }
                if ui.add_enabled(self.splitter.can_redo(), action_button(self.text("Wiederholen", "Redo")).small())
                    .on_hover_text(self.language.btn_redo()).clicked()
                {
                    self.undo_marker_edit(ui.ctx(), true);
                }
                ui.separator();
                ui.spacing_mut().slider_width = 48.0;
                ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0)
                    .text("Zoom").suffix("×"));
                let gain = self.text("Pegel", "Gain");
                ui.add(egui::Slider::new(&mut self.waveform_vertical_zoom, 1.0..=16.0)
                    .text(gain).suffix("×"));
                if ui.add(compact_button("1:1")).on_hover_text(self.text("Ganzes Band anzeigen", "Fit entire tape")).clicked() {
                    self.waveform_zoom = 1.0;
                    self.waveform_vertical_zoom = 1.0;
                    self.waveform_scroll = 0.0;
                }
            });

            let mut selected_marker = self.selected_marker_idx;
            let interaction = render_stereo_waveform(
                ui,
                self.waveform_cache.for_view(self.waveform_zoom as f64),
                total_sec,
                cur_sec,
                self.player.is_playing(),
                &self.splitter.markers,
                &self.splitter.pause_indices,
                self.pause_preview.as_ref().map_or(&[], |proposal| proposal.markers.as_slice()),
                &mut selected_marker,
                self.selected_region,
                &mut self.waveform_zoom,
                &mut self.waveform_vertical_zoom,
                waveform_height,
                &mut self.waveform_scroll,
                self.active_edit_tool,
                self.player.is_auditioning(),
                self.hover_audition_time,
                false,
            );

            if let Some(index) = selected_marker.filter(|&i| self.selected_marker_idx != Some(i)) {
                self.select_marker(index);
            }
            if let Some((r_start, r_end)) = interaction.new_selected_region {
                self.set_scope_region((r_start, r_end));
            }
            if let Some(target_sec) = interaction.seek_to_seconds {
                if has_audio {
                    self.player.seek_seconds(target_sec);
                }
            }
            if let Some(new_marker_sec) = interaction.add_marker_at_seconds {
                if has_audio {
                    self.add_marker(new_marker_sec);
                }
            }
            if let Some((idx, time)) = interaction.dragged_marker {
                self.preview_marker_drag(idx, time);
            }
            if let Some(index) = interaction.delete_marker_idx {
                self.request_marker_delete(index);
            }
            if let Some(segment) = interaction.clicked_segment_idx {
                self.selected_segment_idx = Some(segment);
                self.selected_marker_idx = segment.checked_sub(1);
                self.selected_region = None;
            }

            self.update_hover_audition(ui, &interaction, has_audio);

            // Bottom bar: Directly adjustable Pause Detection parameters + Proposal actions
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.spacing_mut().slider_width = 50.0;
                ui.label(RichText::new(self.text("Pausen:", "Pauses:")).color(p.text_secondary).small());
                let thresh_lbl = self.text("dB", "dB");
                let thresh_tip = self.text("Pegelschwelle für Stille zwischen Songs (Vinyl: ca. -38 dB, Kassette: ca. -45 dB)", "Silence threshold between songs");
                let min_lbl = self.text("Min", "Min");
                let min_tip = self.text("Mindestdauer einer Pause in Sekunden", "Minimum pause duration in seconds");
                let skip_lbl = self.text("Als Pause", "As gap");
                let skip_tip = self.text("Erkannte Pausen als ausgelassene Segmente markieren", "Mark detected pauses as skipped gap segments");
                let mut p_changed = false;
                if ui.add(egui::Slider::new(&mut self.pause_threshold_db, -65.0..=-25.0)
                    .step_by(1.0)
                    .text(thresh_lbl))
                    .on_hover_text(thresh_tip)
                    .changed()
                {
                    p_changed = true;
                }
                if ui.add(egui::Slider::new(&mut self.pause_min_seconds, 0.5..=5.0)
                    .step_by(0.1)
                    .text(min_lbl)
                    .suffix("s"))
                    .on_hover_text(min_tip)
                    .changed()
                {
                    p_changed = true;
                }
                if ui.checkbox(&mut self.pause_as_gaps, skip_lbl)
                    .on_hover_text(skip_tip)
                    .changed()
                {
                    p_changed = true;
                }
                if p_changed {
                    self.save_config();
                }

                if ui.add_enabled(has_audio, action_button(self.text("🔍 Suchen", "🔍 Detect")).small()).clicked() {
                    self.preview_pauses(ui.ctx(), self.pause_as_gaps);
                }

                if let Some((cuts, gaps)) = self.pause_preview.as_ref().map(|p| (p.markers.len(), p.pause_indices.len())) {
                    ui.separator();
                    ui.colored_label(
                        p.amber,
                        RichText::new(format!("{cuts} Schnitte ({gaps}⏸)")).small(),
                    );
                    if ui.add(compact_primary_button(ui, self.text("✓ Übernehmen", "✓ Apply")))
                        .on_hover_text(self.text("Ersetzt die Trackliste. Mit ⌘Z rückgängig machen.", "Replaces the track list. Undo with ⌘Z."))
                        .clicked()
                    {
                        let before = self.splitter.snapshot();
                        self.splitter.restore(self.pause_preview.take().unwrap());
                        self.splitter.commit_edit(before);
                        self.reset_editor_selection();
                    }
                    if ui.add(compact_button("×")).on_hover_text(self.text("Vorschlag verwerfen", "Discard proposal")).clicked() {
                        self.pause_preview = None;
                    }
                }

                if ui.add_enabled(has_audio && !self.splitter.markers.is_empty(),
                    action_button(self.text("Alle löschen", "Clear all")).small()).clicked()
                {
                    self.splitter.clear_markers();
                    self.selected_marker_idx = None;
                    self.selected_segment_idx = None;
                }
            });
        });
    }

    fn render_detail_panel(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64) {
        let p = palette(ui.ctx());
        let marker_count = self.splitter.markers.len();
        let active_marker = self.selected_marker_idx
            .and_then(|idx| self.splitter.markers.get(idx).copied().map(|t| (idx, t)));

        let radius = self.scope_radius_sec.clamp(0.1, 15.0);
        let center_sec = self.scope_locked_center
            .or_else(|| active_marker.map(|(_, t)| t))
            .unwrap_or(cur_sec)
            .clamp(0.0, total_sec);

        let active_region = self.selected_region.unwrap_or_else(|| {
            let start = (center_sec - radius).max(0.0);
            let end = (center_sec + radius).min(total_sec).max(start + 0.05);
            (start, end)
        });

        let samples_l = self.player.get_samples_l();
        let samples_r = self.player.get_samples_r();
        let rate = self.player.sample_rate();

        // Strictly max 3 transients around the active marker (or playhead)
        let transient_anchor = active_marker.map_or(center_sec, |(_, t)| t);
        let search_radius = ((active_region.1 - active_region.0) * 0.5).clamp(0.25, 4.0);
        let quantized_anchor = (transient_anchor * 20.0).round() / 20.0;

        let transients = if self.show_transients && self.marker_drag.is_none() {
            let sensitivity = self.transient_sensitivity;
            if !self.transient_cache.as_ref().is_some_and(|cache| {
                (cache.marker_time - quantized_anchor).abs() < 1e-3
                    && (cache.radius - search_radius).abs() < 1e-3
                    && (cache.sensitivity - sensitivity).abs() < 1e-3
            }) {
                let detected = crate::splitter::detect_marker_transients(
                    &samples_l,
                    &samples_r,
                    rate,
                    transient_anchor,
                    search_radius,
                    sensitivity,
                );
                self.transient_cache = Some(TransientCache {
                    marker_time: quantized_anchor,
                    radius: search_radius,
                    sensitivity,
                    points: detected.into(),
                });
            }
            self.transient_cache.as_ref().unwrap().points.clone()
        } else if self.show_transients {
            self.transient_cache.as_ref().map_or_else(|| std::sync::Arc::from([]), |c| c.points.clone())
        } else {
            std::sync::Arc::from([])
        };

        ui.scope(|ui| {
            panel_title(ui, self.text("Präzisions-Schnitt-Lupe", "Precision Cut Magnifier"));
            ui.spacing_mut().item_spacing.y = 4.0;
            // Row 1: Marker selector + Timecode editor + Fine Nudge buttons + Delete (fits on 1 line in 480px)
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);
                if let Some((m_idx, m_time)) = active_marker {
                    if ui.add_enabled(m_idx > 0, transport_button(ui, "◀", false))
                        .on_hover_text(self.text("Vorherige Schnittmarke", "Previous marker"))
                        .clicked()
                    {
                        self.select_marker(m_idx - 1);
                    }
                    ui.label(
                        RichText::new(format!("T{:02} · {}/{}", m_idx + 2, m_idx + 1, marker_count))
                            .strong()
                            .monospace()
                            .small()
                            .color(p.amber),
                    );
                    if ui.add_enabled(m_idx + 1 < marker_count, transport_button(ui, "▶", false))
                        .on_hover_text(self.text("Nächste Schnittmarke", "Next marker"))
                        .clicked()
                    {
                        self.select_marker(m_idx + 1);
                    }

                    let mut seconds = m_time;
                    let edit = egui::DragValue::new(&mut seconds)
                        .range(0.0..=total_sec)
                        .speed(0.002)
                        .custom_formatter(|s, _| timecode(s))
                        .custom_parser(parse_timecode)
                        .update_while_editing(false);
                    if ui.add(edit).on_hover_text(self.text("Exakte Schnittzeit bearbeiten", "Edit exact cut time")).changed() {
                        let target = self.snap_time_if_enabled(seconds);
                        if ui.input(|input| input.pointer.primary_down() && input.pointer.is_decidedly_dragging()) {
                            self.preview_marker_drag(m_idx, target);
                        } else {
                            self.splitter.move_marker(m_idx, target, total_sec, rate);
                        }
                    }

                    for (label, delta) in [
                        ("−25ms", -0.025),
                        ("−2ms", -0.002),
                        ("+2ms", 0.002),
                        ("+25ms", 0.025),
                    ] {
                        if ui.add(compact_button(label))
                            .on_hover_text(self.text("Schnittmarke fein verschieben (Pfeiltasten ←/→)", "Fine-nudge cut marker (Arrow keys ←/→)"))
                            .clicked()
                        {
                            self.nudge_marker(m_idx, delta, total_sec);
                        }
                    }

                    if ui.add(compact_button(self.text("Löschen", "Delete")))
                        .on_hover_text(self.text("Schnittmarke entfernen (⌫)", "Delete cut marker (⌫)"))
                        .clicked()
                    {
                        self.request_marker_delete(m_idx);
                    }
                } else {
                    ui.label(RichText::new(self.text("Klicke in die Lupe oder drücke M", "Click inside scope or press M")).color(p.text_secondary).small());
                    if ui.add(compact_button(self.text("Marke setzen", "Place marker"))).clicked() {
                        self.add_marker(cur_sec);
                    }
                }
            });

            // Row 2: Window radius pills + Auto-Gain + Transients (max 3) & sensitivity (fits on 1 line in 480px)
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);
                for (rad, lbl) in [
                    (0.25_f64, "±0.25s"),
                    (0.75, "±0.75s"),
                    (1.5, "±1.5s"),
                    (3.0, "±3s"),
                    (5.0, "±5s"),
                ] {
                    let is_sel = self.selected_region.is_none() && (self.scope_radius_sec - rad).abs() < 0.05;
                    if ui.selectable_label(is_sel, RichText::new(lbl).small()).clicked() {
                        self.scope_radius_sec = rad;
                        self.selected_region = None;
                    }
                }
                ui.separator();
                let ag_lbl = self.text("Auto-dB", "Auto-dB");
                ui.checkbox(&mut self.scope_auto_gain, RichText::new(ag_lbl).small());
                if !self.scope_auto_gain {
                    ui.add(egui::DragValue::new(&mut self.scope_gain_db).range(-24.0..=36.0).speed(0.5).suffix("dB"));
                }
                ui.separator();
                let trans_lbl = self.text("⚡Max 3", "⚡Max 3");
                let trans_tip = self.text("Zeigt maximal die 3 stärksten Song-Einsätze rund um den Schnittmarker", "Shows at most the 3 strongest song attacks around the cut marker");
                ui.toggle_value(&mut self.show_transients, RichText::new(trans_lbl).small())
                    .on_hover_text(trans_tip);
                if self.show_transients {
                    ui.spacing_mut().slider_width = 36.0;
                    ui.add(egui::Slider::new(&mut self.transient_sensitivity, 0.4..=2.5)
                        .show_value(false))
                        .on_hover_text(self.language.label_sensitivity());
                }
            });

            let span = (active_region.1 - active_region.0).max(0.05);
            let scope_action = crate::ui::waveform_widget::render_detail_magnifier_scope(
                ui,
                self.waveform_cache.for_view(total_sec / span),
                &samples_l,
                &samples_r,
                rate,
                total_sec,
                active_region,
                &self.splitter.markers,
                self.selected_marker_idx,
                self.scope_auto_gain,
                self.scope_gain_db,
                &transients,
                self.show_transients,
                1.0,
                &mut self.scope_scroll,
                cur_sec,
                self.player.is_playing(),
            );

            if let Some(factor) = scope_action.zoom_delta_factor {
                self.selected_region = None;
                self.scope_radius_sec = (self.scope_radius_sec * f64::from(factor)).clamp(0.15, 8.0);
            }
            if let Some(t_seek) = scope_action.seek_to_time {
                self.player.seek_seconds(t_seek);
            }
            if let Some(t_trans) = scope_action.clicked_transient_time {
                let target = self.snap_time_if_enabled(t_trans);
                if let Some(m_idx) = self.selected_marker_idx {
                    let before = self.splitter.snapshot();
                    if self.splitter.move_marker(m_idx, target, total_sec, rate) {
                        self.splitter.commit_edit(before);
                        self.scope_locked_center = None;
                    }
                } else {
                    self.add_marker(target);
                }
                self.player.seek_seconds(target);
            }
            if let Some(s_idx) = scope_action.selected_marker_idx {
                self.select_marker(s_idx);
            }
            if let Some((idx, time)) = scope_action.dragged_marker {
                if self.scope_locked_center.is_none() {
                    self.scope_locked_center = Some(center_sec);
                }
                self.preview_marker_drag(idx, time);
            }
            if let Some((idx, time)) = scope_action.move_marker_to {
                let target = self.snap_time_if_enabled(time);
                let before = self.splitter.snapshot();
                if self.splitter.move_marker(idx, target, total_sec, rate) {
                    self.splitter.commit_edit(before);
                }
                self.player.seek_seconds(target);
            }
            if let Some(new_t) = scope_action.create_marker_at {
                self.add_marker(new_t);
            }

            // Row 3: Pre-Roll / Post-Roll / Transition Audition + 1-click Transient Snap buttons
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding = Vec2::new(6.0, 2.0);
                let ref_t = active_marker.map_or(cur_sec, |(_, t)| t);
                if ui.add(compact_button(self.text("▶ Davor 2s", "▶ Before 2s")))
                    .on_hover_text(self.text("Spielt die letzten 2 Sekunden vor dem Schnitt (Ende des vorherigen Tracks)", "Plays 2 seconds before the cut"))
                    .clicked()
                {
                    self.play_recording_range((ref_t - 2.0).max(0.0), ref_t);
                }
                if ui.add(compact_button(self.text("▶ Danach 2s", "▶ After 2s")))
                    .on_hover_text(self.text("Spielt die ersten 2 Sekunden ab dem Schnitt (Start des neuen Tracks)", "Plays 2 seconds starting at the cut"))
                    .clicked()
                {
                    self.play_recording_range(ref_t, (ref_t + 2.0).min(total_sec));
                }
                if ui.add(compact_button(self.text("▶ Übergang", "▶ Transition")))
                    .on_hover_text(self.text("Spielt über die Schnittgrenze hinweg (±1.5s)", "Plays across the cut boundary (±1.5s)"))
                    .clicked()
                {
                    self.play_recording_range((ref_t - 1.5).max(0.0), (ref_t + 1.5).min(total_sec));
                }
                if self.show_transients && !transients.is_empty() {
                    ui.separator();
                    for (t_i, tp) in transients.iter().take(crate::splitter::MAX_MARKER_TRANSIENTS).enumerate() {
                        let delta = tp.time_sec - ref_t;
                        let btn_text = format!("⚡{}{:+.2}s", t_i + 1, delta);
                        if ui.add(compact_button(RichText::new(btn_text).monospace().small().color(p.amber)))
                            .on_hover_text(self.text("Schnittmarke exakt auf diesen Transient-Einsatz setzen", "Snap cut marker to this transient attack"))
                            .clicked()
                        {
                            let target = self.snap_time_if_enabled(tp.time_sec);
                            if let Some((m_idx, _)) = active_marker {
                                let before = self.splitter.snapshot();
                                if self.splitter.move_marker(m_idx, target, total_sec, rate) {
                                    self.splitter.commit_edit(before);
                                    self.scope_locked_center = None;
                                }
                            } else {
                                self.add_marker(target);
                            }
                            self.play_recording_range(target, (target + 1.5).min(total_sec));
                        }
                    }
                }
            });
        });
    }

    fn refresh_noise_presets(&mut self) {
        self.saved_noise_presets = NoisePresetManager::list_presets();
        if let Some(ref active) = self.active_noise_profile {
            self.selected_preset_idx = self.saved_noise_presets.iter().position(|p| p.name == active.name);
        }
    }

    fn active_selection_or_window(&self, total_sec: f64, cur_sec: f64) -> (f64, f64) {
        if let Some((s, e)) = self.selected_region {
            if e > s + 0.05 {
                return (s.clamp(0.0, total_sec), e.clamp(0.0, total_sec));
            }
        }
        let start = cur_sec.clamp(0.0, (total_sec - 0.5).max(0.0));
        let end = (start + 2.0).min(total_sec);
        (start, end)
    }

    fn learn_noise_from_region(&mut self, start_sec: f64, end_sec: f64) {
        let sr = self.player.sample_rate();
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let len = left.len().min(right.len());
        if sr == 0 || len == 0 {
            return;
        }
        let s_idx = ((start_sec * f64::from(sr)).round() as usize).min(len);
        let e_idx = ((end_sec * f64::from(sr)).round() as usize).min(len);
        if e_idx <= s_idx {
            return;
        }
        let slice_l = left[s_idx..e_idx].to_vec();
        let slice_r = right[s_idx..e_idx].to_vec();
        let default_name = format!(
            "{} ({})",
            self.tape_name.trim(),
            self.media_source.label(self.language)
        );
        match NoiseProfile::learn_from_samples(
            &slice_l,
            &slice_r,
            sr,
            &default_name,
            self.media_source.label(Language::En),
        ) {
            Ok(profile) => {
                if self.preset_save_name.trim().is_empty() {
                    self.preset_save_name = default_name;
                }
                self.active_noise_samples = Some((slice_l, slice_r, sr));
                self.active_noise_profile = Some(profile.clone());
                self.selected_preset_idx = None;
                self.denoiser_params.enabled = true;
                self.player.set_noise_profile(Some(profile));
                self.player.set_denoiser_params(self.denoiser_params.clone());
                self.ui_error_message = None;
                self.audio_status = Some(
                    self.text(
                        "✓ Rauschmuster aus Auswahl gelernt & Vorhören aktiviert",
                        "✓ Learned noise print from selection & enabled audition",
                    )
                    .to_string(),
                );
            }
            Err(err) => {
                self.ui_error_message = Some(err);
            }
        }
    }

    fn save_current_noise_preset(&mut self) {
        let name = if self.preset_save_name.trim().is_empty() {
            format!("{} Rauschen", self.tape_name.trim())
        } else {
            self.preset_save_name.trim().to_string()
        };
        let media = self.media_source.label(Language::En).to_string();

        let res = if let Some((ref sl, ref sr, rate)) = self.active_noise_samples {
            NoisePresetManager::save_preset(&name, &media, sl, sr, rate)
        } else if let Some((r_start, r_end)) = self.selected_region {
            let rate = self.player.sample_rate();
            let left = self.player.get_samples_l();
            let right = self.player.get_samples_r();
            let len = left.len().min(right.len());
            let s_idx = ((r_start * f64::from(rate)).round() as usize).min(len);
            let e_idx = ((r_end * f64::from(rate)).round() as usize).min(len);
            if e_idx > s_idx {
                let sl = left[s_idx..e_idx].to_vec();
                let sr_vec = right[s_idx..e_idx].to_vec();
                self.active_noise_samples = Some((sl.clone(), sr_vec.clone(), rate));
                NoisePresetManager::save_preset(&name, &media, &sl, &sr_vec, rate)
            } else {
                Err(self.text("Bitte zuerst einen Rausch-Bereich auswählen.", "Please select a noise region first.").into())
            }
        } else {
            Err(self.text("Kein Rausch-Sample zum Speichern vorhanden.", "No noise sample available to save.").into())
        };

        match res {
            Ok((profile, wav_path)) => {
                self.active_noise_profile = Some(profile.clone());
                self.player.set_noise_profile(Some(profile));
                self.denoiser_params.enabled = true;
                self.player.set_denoiser_params(self.denoiser_params.clone());
                self.refresh_noise_presets();
                self.ui_error_message = None;
                self.audio_status = Some(format!(
                    "✓ Preset gespeichert (App & Logic AU): {}",
                    wav_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(err) => self.ui_error_message = Some(err),
        }
    }

    fn edit_recording(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        edit: RecordingEdit,
    ) {
        if self.job.is_some() || self.current_audio_path.is_none() || self.player.total_samples() == 0 {
            return;
        }
        let directory = self.current_project_path.as_deref()
            .and_then(Path::parent)
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.session_project_dir());
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let sample_rate = self.player.sample_rate();
        self.song_editor.finish_edit(&mut self.splitter);
        let markers = self.splitter.snapshot();
        self.pause_audio_for_edit();
        self.start_job(ctx, label, move || {
            edit.execute(&directory, &left, &right, sample_rate, markers)
                .map(JobResult::AudioEdited)
        });
    }

    fn cut_or_trim_recording(&mut self, ctx: &egui::Context, keep_selection: bool) {
        let Some((start, end)) = self.selected_region else {
            self.ui_error_message = Some(self.text(
                "Bitte zuerst einen Abschnitt in der Wellenform markieren.",
                "Select a range in the waveform first.",
            ).into());
            return;
        };
        let (edit, label) = if keep_selection {
            (RecordingEdit::Keep(start..end), self.text("Aufnahme wird getrimmt …", "Trimming recording …"))
        } else {
            (RecordingEdit::Remove(start..end), self.text("Auswahl wird ausgeschnitten …", "Removing selection …"))
        };
        self.edit_recording(ctx, label, edit);
    }

    fn bake_denoiser_into_recording(&mut self, ctx: &egui::Context) {
        let label = self.text("Rausch- und DC-Filter wird eingerechnet …", "Rendering noise and DC filter …");
        self.apply_recording_filter(ctx, label, None);
    }

    fn bake_denoiser_into_track(&mut self, ctx: &egui::Context, segment_idx: usize) {
        let tracks = self.splitter.get_tracks(self.player.total_seconds());
        let Some(track) = tracks.iter().find(|track| track.segment_index == segment_idx && !track.is_pause) else { return; };
        let label = self.text("Rauschfilter für Track wird eingerechnet …", "Rendering track noise filter …");
        self.apply_recording_filter(ctx, label, Some((segment_idx, track.start_seconds..track.end_seconds)));
    }

    fn apply_recording_filter(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        track: Option<(usize, std::ops::Range<f64>)>,
    ) {
        self.edit_recording(ctx, label, RecordingEdit::Restore {
            track,
            profile: self.active_noise_profile.clone(),
            params: self.denoiser_params.clone(),
        });
    }

    fn render_denoiser_and_cut_panel(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64) {
        let p = palette(ui.ctx());
        let (sel_start, sel_end) = self.active_selection_or_window(total_sec, cur_sec);
        let sel_dur = (sel_end - sel_start).max(0.0);

        ui.scope(|ui| {
                panel_title(ui, self.text("Effekte · Rausch- & DC-Filter", "Effects · Noise & DC filter"));
                ui.spacing_mut().item_spacing.y = 4.0;

                // -------------------------------------------------------------
                // ROW 1: AUFNAHME IN EINER NEUEN WAV-VERSION SCHNEIDEN
                // -------------------------------------------------------------
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);

                    let mut r0 = sel_start;
                    let mut r1 = sel_end;
                    let r0_changed = ui
                        .add(
                            egui::DragValue::new(&mut r0)
                                .range(0.0..=r1.max(0.05) - 0.05)
                                .speed(0.05)
                                .custom_formatter(|s, _| timecode(s))
                                .custom_parser(parse_timecode),
                        )
                        .on_hover_text(self.text("Start der Bereichsauswahl", "Selection start"))
                        .changed();
                    ui.label(RichText::new("–").color(p.text_muted).small());
                    let r1_changed = ui
                        .add(
                            egui::DragValue::new(&mut r1)
                                .range((r0 + 0.05).min(total_sec)..=total_sec)
                                .speed(0.05)
                                .custom_formatter(|s, _| timecode(s))
                                .custom_parser(parse_timecode),
                        )
                        .on_hover_text(self.text("Ende der Bereichsauswahl", "Selection end"))
                        .changed();
                    if r0_changed || r1_changed {
                        self.selected_region = Some((r0.min(r1 - 0.05), r1.max(r0 + 0.05)));
                    }

                    if ui.add(compact_button(format!("▶{:.1}s", sel_dur)))
                        .on_hover_text(self.text(
                            "Ausgewählten Bereich vorhören",
                            "Audition selected range",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.play_recording_range(sel_start, sel_end);
                    }

                    if ui
                        .add_enabled(
                            sel_dur >= 0.05,
                            action_button(
                                RichText::new(self.text("✂ Löschen", "✂ Delete")).color(p.amber).small(),
                            )
                            .small(),
                        )
                        .on_hover_text(self.text(
                            "Entfernt die Auswahl in einer neuen WAV-Version und passt die Marker an.",
                            "Removes the selection in a new WAV revision and adjusts the markers.",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.cut_or_trim_recording(ui.ctx(), false);
                    }

                    if ui
                        .add_enabled(
                            sel_dur >= 0.1 && sel_dur < total_sec - 0.05,
                            action_button(RichText::new(self.text("⬚ Trim", "⬚ Crop")).small()).small(),
                        )
                        .on_hover_text(self.text(
                            "Behält nur die Auswahl in einer neuen WAV-Version.",
                            "Keeps only the selection in a new WAV revision.",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.cut_or_trim_recording(ui.ctx(), true);
                    }

                });

                // -------------------------------------------------------------
                // ROW 2: RAUSCH-SAMPLE LERNEN & PRESET-VERWALTUNG (WAV + JSON)
                // -------------------------------------------------------------
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);

                    let learn_lbl = self.text("🎯 Rauschen lernen", "🎯 Learn Noise");
                    if ui
                        .add(compact_primary_button(ui, learn_lbl))
                        .on_hover_text(self.text(
                            "Analysiert das Bandrauschen & DC-Signal im gewählten Bereich und aktiviert das Vorhören",
                            "Analyzes tape hiss & DC offset in the selected region and activates live audition",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.learn_noise_from_region(sel_start, sel_end);
                    }

                    let active_preset_label = self
                        .active_noise_profile
                        .as_ref()
                        .map(|p| format!("{} ({:.0}dB)", p.name, p.rms_db_l.max(p.rms_db_r)))
                        .unwrap_or_else(|| self.text("Kein Preset", "No preset").into());

                    egui::ComboBox::from_id_salt("noise_preset_combo")
                        .width(115.0)
                        .truncate()
                        .selected_text(RichText::new(active_preset_label).small())
                        .show_ui(ui, |ui| {
                            for (idx, preset) in self.saved_noise_presets.clone().into_iter().enumerate() {
                                let label = format!(
                                    "{} · {} ({:.1} dB)",
                                    preset.name,
                                    preset.media_source,
                                    preset.rms_db_l.max(preset.rms_db_r)
                                );
                                if ui
                                    .selectable_label(self.selected_preset_idx == Some(idx), label)
                                    .clicked()
                                {
                                    self.selected_preset_idx = Some(idx);
                                    self.preset_save_name = preset.name.clone();
                                    self.active_noise_profile = Some(preset.clone());
                                    if let Some(ref wav_name) = preset.wav_filename {
                                        let wav_path = NoisePresetManager::default_presets_dir().join(wav_name);
                                        if let Ok(decoded) = AudioPlayer::decode_file(&wav_path) {
                                            self.active_noise_samples =
                                                Some((decoded.left, decoded.right, decoded.sample_rate));
                                        }
                                    }
                                    self.denoiser_params.enabled = true;
                                    self.player.set_noise_profile(Some(preset));
                                    self.player.set_denoiser_params(self.denoiser_params.clone());
                                }
                            }
                        });

                    let preset_hint = self.text("Preset-Name …", "Preset name …");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.preset_save_name)
                            .desired_width(86.0)
                            .margin(egui::Margin::symmetric(4.0, 2.0))
                            .hint_text(preset_hint),
                    );

                    if ui
                        .add_enabled(
                            self.active_noise_profile.is_some() || self.selected_region.is_some(),
                            action_button(RichText::new(self.text("💾 Preset", "💾 Save")).small()).small(),
                        )
                        .on_hover_text(self.text(
                            "Speichert die Rausch-Samples als WAV + Spektralprofil für andere Aufnahmen & das Logic Pro AudioUnit",
                            "Saves noise samples as WAV + spectral profile for other recordings & Logic Pro AudioUnit",
                        ))
                        .clicked()
                    {
                        self.save_current_noise_preset();
                    }

                    if ui.add(compact_button("📂"))
                        .on_hover_text(self.text(
                            "Externe Rausch-WAV-Datei als Profil laden",
                            "Load external noise WAV file as profile",
                        ))
                        .clicked()
                    {
                        let dir = NoisePresetManager::default_presets_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        if let Some(path) = rfd::FileDialog::new()
                            .set_directory(&dir)
                            .add_filter("WAV Audio", &["wav"])
                            .pick_file()
                        {
                            match NoisePresetManager::load_profile_from_wav(&path) {
                                Ok(prof) => {
                                    if let Ok(dec) = AudioPlayer::decode_file(&path) {
                                        self.active_noise_samples = Some((dec.left, dec.right, dec.sample_rate));
                                    }
                                    self.preset_save_name = prof.name.clone();
                                    self.active_noise_profile = Some(prof.clone());
                                    self.denoiser_params.enabled = true;
                                    self.player.set_noise_profile(Some(prof));
                                    self.player.set_denoiser_params(self.denoiser_params.clone());
                                    self.refresh_noise_presets();
                                }
                                Err(e) => self.ui_error_message = Some(e),
                            }
                        }
                    }

                    if let Some(idx) = self.selected_preset_idx {
                        if ui.add(compact_button("🗑"))
                            .on_hover_text(self.text("Gewähltes Preset löschen", "Delete selected preset"))
                            .clicked()
                        {
                            if let Some(prof) = self.saved_noise_presets.get(idx).cloned() {
                                let _ = NoisePresetManager::delete_preset_in_dir(
                                    &NoisePresetManager::default_presets_dir(),
                                    &prof,
                                );
                                self.selected_preset_idx = None;
                                self.refresh_noise_presets();
                            }
                        }
                    }
                });

                // -------------------------------------------------------------
                // ROW 3: ECHTZEIT-VORHÖREN, REGLER & IN AUFNAHME EINRECHNEN
                // -------------------------------------------------------------
                let mut params_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.spacing_mut().button_padding = Vec2::new(4.0, 2.0);

                    let active_lbl = if self.denoiser_params.enabled {
                        self.text("🟢 AN", "🟢 ON")
                    } else {
                        self.text("⚪ AUS", "⚪ OFF")
                    };
                    if ui
                        .selectable_label(self.denoiser_params.enabled, RichText::new(active_lbl).small().strong())
                        .on_hover_text(self.text(
                            "Schaltet den Echtzeit-Rausch- und DC-Filter für die Wiedergabe ein/aus",
                            "Toggles real-time noise and DC filtering during playback",
                        ))
                        .clicked()
                    {
                        self.denoiser_params.enabled = !self.denoiser_params.enabled;
                        params_changed = true;
                    }

                    for (mode, label, tip) in [
                        (
                            AuditionMode::Wet,
                            self.text("🎧Wet", "🎧Wet"),
                            self.text("Spielt das entrauschte & DC-bereinigte Signal", "Plays the denoised & DC-cleaned signal"),
                        ),
                        (
                            AuditionMode::Bypass,
                            "A/B",
                            self.text("Unbearbeitetes Signal zum direkten Vergleich", "Unprocessed signal for instant A/B comparison"),
                        ),
                        (
                            AuditionMode::DeltaNoiseOnly,
                            self.text("🔍Δ", "🔍Δ"),
                            self.text(
                                "Spielt NUR das entfernte Rauschen ab – ideal zur Kontrolle, dass keine Musik angegriffen wird",
                                "Plays ONLY the removed noise – ideal to verify no music harmonics are stripped",
                            ),
                        ),
                    ] {
                        if ui
                            .add_enabled(
                                self.denoiser_params.enabled,
                                egui::SelectableLabel::new(
                                    self.denoiser_params.audition_mode == mode,
                                    RichText::new(label).small(),
                                ),
                            )
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.denoiser_params.audition_mode = mode;
                            params_changed = true;
                        }
                    }

                    let dc_tip = self.text(
                        "Entfernt Gleichspannungs-Offsets (DC) und tieffrequentes Band-/Plattenrumpeln (20 Hz)",
                        "Removes DC offset and low-frequency tape/turntable rumble (20 Hz)",
                    );
                    if ui
                        .checkbox(&mut self.denoiser_params.dc_filter_enabled, RichText::new("DC").small())
                        .on_hover_text(dc_tip)
                        .changed()
                    {
                        params_changed = true;
                    }

                    let red_tip = self.text(
                        "Stärke der Rauschunterdrückung in dB (12–18 dB empfohlen für natürlichen Bandklang)",
                        "Noise reduction amount in dB (12–18 dB recommended for natural tape tone)",
                    );
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.denoiser_params.reduction_db)
                                .range(0.0..=36.0)
                                .speed(0.2)
                                .prefix("-")
                                .suffix("dB"),
                        )
                        .on_hover_text(red_tip)
                        .changed()
                    {
                        params_changed = true;
                    }

                    let sens_tip = self.text(
                        "Empfindlichkeit (Threshold) gegenüber dem gelernten Rauschprofil",
                        "Sensitivity (Threshold) relative to the learned noise profile",
                    );
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.denoiser_params.sensitivity)
                                .range(0.5..=3.5)
                                .speed(0.02)
                                .suffix("×"),
                        )
                        .on_hover_text(sens_tip)
                        .changed()
                    {
                        params_changed = true;
                    }

                    let smooth_tip = self.text(
                        "Spektrale und zeitliche Glättung gegen Artefakte (Musical Noise)",
                        "Spectral and temporal smoothing to prevent musical noise artifacts",
                    );
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.denoiser_params.smoothing)
                                .range(0.0..=0.95)
                                .speed(0.01)
                                .prefix("S:"),
                        )
                        .on_hover_text(smooth_tip)
                        .changed()
                    {
                        params_changed = true;
                    }

                    let tracks = self.splitter.get_tracks(total_sec);
                    let target_seg = self.selected_segment_idx.or_else(|| {
                        tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds).map(|t| t.segment_index)
                    });
                    let selected_track = target_seg.and_then(|idx| tracks.iter().find(|t| t.segment_index == idx && !t.is_pause));
                    let can_apply = self.active_noise_profile.is_some() || self.denoiser_params.dc_filter_enabled;

                    let apply_track = selected_track.map_or_else(
                        || self.text("In Track einrechnen", "Apply to track").to_string(),
                        |track| format!("{} {:02}", self.text("Anwenden auf Track", "Apply to Track"), track.index),
                    );
                    let apply_hint = self.text(
                        "Rendert die Bearbeitung in eine neue WAV-Version.",
                        "Renders the edit to a new WAV revision.",
                    );
                    if ui.add_enabled(can_apply && selected_track.is_some(), action_button(apply_track).small())
                        .on_hover_text(apply_hint).clicked()
                    {
                        if let Some(track) = selected_track {
                            self.bake_denoiser_into_track(ui.ctx(), track.segment_index);
                        }
                    }
                    if ui.add_enabled(can_apply, action_button(self.text("In Aufnahme einrechnen", "Apply to recording")).small())
                        .on_hover_text(apply_hint).clicked()
                    {
                        self.bake_denoiser_into_recording(ui.ctx());
                    }
                });

                if params_changed {
                    self.player.set_denoiser_params(self.denoiser_params.clone());
                }

                // -------------------------------------------------------------
                // ROW 4: SPEKTRALES RAUSCHPROFIL-DISPLAY & STATUS
                // -------------------------------------------------------------
                let desired_h = (ui.available_height() - 16.0).clamp(28.0, 110.0);
                let (spec_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), desired_h), egui::Sense::hover());
                let painter = ui.painter();
                draw_recessed_well(painter, spec_rect, 4.0, p.bg_inset);

                if let Some(ref prof) = self.active_noise_profile {
                    let num_bars = 48_usize;
                    let pad_x = 8.0;
                    let inner_w = (spec_rect.width() - pad_x * 2.0).max(48.0);
                    let step_w = inner_w / num_bars as f32;
                    let bar_w = (step_w * 0.72).max(2.0);
                    let bins = prof.noise_power_l.len().min(prof.noise_power_r.len());

                    for b in 0..num_bars {
                        let t0 = (b as f32) / (num_bars as f32);
                        let t1 = ((b + 1) as f32) / (num_bars as f32);
                        let k0 = ((bins as f32).powf(t0).floor() as usize).clamp(1, bins.saturating_sub(1));
                        let k1 = ((bins as f32).powf(t1).ceil() as usize).clamp(k0 + 1, bins);

                        let mut sum_p = 0.0_f32;
                        for k in k0..k1 {
                            sum_p += 0.5 * (prof.noise_power_l[k] + prof.noise_power_r[k]);
                        }
                        let avg_amp = (sum_p / ((k1 - k0).max(1) as f32)).sqrt() / 512.0;
                        let db = crate::audio::meter::linear_to_db(avg_amp);
                        let norm_h = ((db + 90.0) / 70.0).clamp(0.06, 1.0);
                        let bar_h = norm_h * (spec_rect.height() - 8.0);
                        let x = spec_rect.min.x + pad_x + b as f32 * step_w;
                        let bar_rect = egui::Rect::from_min_max(
                            Pos2::new(x, spec_rect.max.y - 3.0 - bar_h),
                            Pos2::new(x + bar_w, spec_rect.max.y - 3.0),
                        );
                        let col = if self.denoiser_params.enabled {
                            p.teal.gamma_multiply(0.72)
                        } else {
                            p.text_muted.gamma_multiply(0.48)
                        };
                        painter.rect_filled(bar_rect, 1.5, col);
                    }

                    let dc_pct = (prof.dc_offset_l.abs().max(prof.dc_offset_r.abs())) * 100.0;
                    let info = if let Some(ref msg) = self.audio_status {
                        format!(
                            "{} · {} ({:.1} dB · DC {:.2}%)",
                            msg,
                            prof.name,
                            prof.rms_db_l.max(prof.rms_db_r),
                            dc_pct
                        )
                    } else {
                        format!(
                            "Profil: {} · RMS {:.1}/{:.1} dBFS · DC {:.2}% · -{:.1} dB",
                            prof.name, prof.rms_db_l, prof.rms_db_r, dc_pct, self.denoiser_params.reduction_db
                        )
                    };
                    painter.text(
                        Pos2::new(spec_rect.min.x + 8.0, spec_rect.min.y + 9.0),
                        egui::Align2::LEFT_CENTER,
                        info,
                        egui::FontId::monospace(9.0),
                        p.teal,
                    );
                } else {
                    let hint = self.audio_status.as_deref().unwrap_or_else(|| {
                        self.text(
                            "Bereich mit Rauschen wählen ('Bereich·R') -> '🎯 Rauschen lernen'",
                            "Select region with hiss ('Range·R') -> '🎯 Learn Noise'",
                        )
                    });
                    painter.text(
                        spec_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        hint,
                        egui::FontId::monospace(9.5),
                        if self.audio_status.is_some() { p.teal } else { p.text_secondary },
                    );
                }
        });
    }

    fn render_track_list(&mut self, ui: &mut Ui, tracks: &[crate::splitter::TrackInfo]) {
        let p = palette(ui.ctx());
        if self.splitter.custom_titles.len() < tracks.len() {
            self.splitter.custom_titles.resize(tracks.len(), String::new());
        }

        let header = if self.batch_titles_open { 170.0 } else { 72.0 };
        let per_page = ((ui.available_height() - header) / 36.0).floor().max(1.0) as usize;
        let pages = tracks.len().div_ceil(per_page).max(1);
        self.track_page = self.track_page.min(pages - 1);

        ui.scope(|ui| {
            ui.horizontal_wrapped(|ui| {
                panel_title(ui, self.text("Trackliste", "Track list"));
                let active_tracks = tracks.iter().filter(|t| !t.is_pause).count();
                let filtered_tracks = tracks.iter().filter(|t| !t.is_pause && t.is_filtered).count();
                if active_tracks > 0 {
                    ui.label(RichText::new(format!("{active_tracks}")).color(p.text_secondary).small());
                }

                ui.label(RichText::new(format!("{filtered_tracks}/{active_tracks} {}", self.text("gefiltert", "filtered"))).small().color(p.text_muted));
                if ui.add_enabled_ui(active_tracks > 0, |ui| bevel_button(ui, self.text("Alle filtern", "Filter all"), BevelStyle::Compact)).inner.clicked() {
                    self.splitter.set_all_tracks_filtered(tracks.len(), true);
                }
                if ui.add_enabled_ui(active_tracks > 0, |ui| bevel_button(ui, self.text("Keine", "None"), BevelStyle::Compact)).inner.clicked() {
                    self.splitter.set_all_tracks_filtered(tracks.len(), false);
                }

                let selected_track = tracks.iter().find(|track| Some(track.segment_index) == self.selected_segment_idx && !track.is_pause);
                if ui.add_enabled_ui(selected_track.is_some(), |ui| bevel_button(ui, self.text("Track bearbeiten →", "Process track →"), BevelStyle::Compact)).inner.clicked() {
                    if let Some(track) = selected_track {
                        self.selected_region = None;
                        self.player.seek_seconds(track.start_seconds);
                        self.console_tab = ConsoleTab::Filter;
                    }
                }

                if bevel_button(ui, self.text("Liste einfügen", "Paste list"), BevelStyle::Compact)
                    .on_hover_text(self.text("Mehrere Tracknamen auf einmal einfügen (1 Zeile pro Track)", "Paste multiple track titles at once (1 line per track)"))
                    .clicked()
                {
                    self.batch_titles_open = !self.batch_titles_open;
                    if self.batch_titles_open && self.batch_titles_text.is_empty() {
                        self.batch_titles_text = tracks.iter()
                            .filter(|t| !t.is_pause)
                            .map(|t| self.splitter.custom_titles.get(t.segment_index).cloned().unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                }
                page_controls(ui, &mut self.track_page, pages, self.language);
            });

            if self.batch_titles_open {
                ui.add_space(4.0);
                let hint_txt = self.text("Zeile 1: Song A\nZeile 2: Song B …", "Line 1: Song A\nLine 2: Song B …");
                let apply_txt = self.text("Übernehmen", "Apply");
                let cancel_txt = self.language.btn_cancel();
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 110.0).max(160.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut self.batch_titles_text)
                            .desired_rows(3)
                            .desired_width(w)
                            .hint_text(hint_txt),
                    );
                    ui.vertical(|ui| {
                        if ui.add(action_button(apply_txt)).clicked() {
                            let lines: Vec<String> = self.batch_titles_text
                                .lines()
                                .map(|l: &str| l.trim().to_string())
                                .filter(|l: &String| !l.is_empty())
                                .collect();
                            let mut line_iter = lines.into_iter();
                            for t in tracks.iter().filter(|t| !t.is_pause) {
                                if let Some(line) = line_iter.next() {
                                    // Strip optional leading track numbers like "01 - " or "1. "
                                    let cleaned = line.trim_start_matches(|c: char| c.is_ascii_digit())
                                        .trim_start_matches(['.', '-', ')', ' ', '\t']);
                                    let final_name = if cleaned.is_empty() { line.as_str() } else { cleaned };
                                    if let Some(slot) = self.splitter.custom_titles.get_mut(t.segment_index) {
                                        *slot = final_name.to_string();
                                    }
                                }
                            }
                            self.batch_titles_open = false;
                        }
                        if ui.add(compact_button(cancel_txt)).clicked() {
                            self.batch_titles_open = false;
                        }
                    });
                });
            }

            ui.add_space(6.0);
            let start = self.track_page * per_page;
            for (row_idx, (index, track)) in tracks.iter().enumerate().skip(start).take(per_page).enumerate() {
                let selected = self.selected_segment_idx == Some(index);
                let row_bg = if selected {
                    p.accent_soft
                } else if row_idx % 2 == 0 {
                    p.bg_card
                } else {
                    p.bg_panel
                };

                egui::Frame::none()
                    .fill(row_bg)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::symmetric(6.0, 3.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let badge_txt = if track.is_pause { "||".into() } else { format!("{:02}", track.index) };
                            if bevel_button_sized(
                                ui,
                                &badge_txt,
                                26.0,
                                if selected { BevelStyle::CompactEngaged } else { BevelStyle::Compact },
                            ).clicked() {
                                self.selected_segment_idx = Some(index);
                                self.selected_marker_idx = index.checked_sub(1);
                                self.player.seek_seconds(track.start_seconds);
                            }

                            // Reserve exact space for the compact right-hand controls so the Track Title field
                            // stays >= 520 pt wide at 800 pt while never overflowing the row width.
                            let right_controls_w = if index > 0 { 242.0 } else { 202.0 };
                            let title_width = (ui.available_width() - right_controls_w).max(140.0);

                            if track.is_pause {
                                ui.add_sized(
                                    [title_width, 22.0],
                                    egui::Label::new(RichText::new(self.language.badge_pause()).color(p.amber).small()),
                                );
                            } else {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut self.splitter.custom_titles[index])
                                        .id(egui::Id::new("track_title_field").with(index))
                                        .desired_width(title_width)
                                        .margin(egui::Margin::symmetric(6.0, 2.0))
                                        .hint_text(format!("Track {:02}", track.index)),
                                );
                                if response.gained_focus() {
                                    self.selected_segment_idx = Some(index);
                                    self.selected_marker_idx = index.checked_sub(1);
                                }
                                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                    if let Some(next) = tracks.iter().skip(index + 1).find(|track| !track.is_pause) {
                                        self.track_page = next.segment_index / per_page;
                                        ui.memory_mut(|mem| mem.request_focus(egui::Id::new("track_title_field").with(next.segment_index)));
                                    }
                                }
                            }

                            let start_tc = RichText::new(timecode(track.start_seconds)).monospace().small();
                            if index > 0 {
                                if ui.selectable_label(selected, start_tc)
                                    .on_hover_text(self.text("Schnittmarke in der Schnitt-Lupe fokussieren", "Focus cut marker in Cut Magnifier"))
                                    .clicked()
                                {
                                    self.select_marker(index - 1);
                                    self.console_tab = ConsoleTab::Edit;
                                    ui.ctx().request_repaint();
                                }
                            } else {
                                ui.label(start_tc.color(p.text_secondary));
                            }

                            ui.label(RichText::new(track.duration_formatted()).monospace().small().color(p.text_secondary));

                            if bevel_button(ui, "▶", BevelStyle::Compact)
                                .on_hover_text(self.text("Track anhören", "Audition track"))
                                .clicked()
                            {
                                self.play_recording_range(track.start_seconds, track.end_seconds);
                            }

                            if !track.is_pause {
                                if bevel_button(ui, "FX", BevelStyle::Compact)
                                    .on_hover_text(self.text("Diesen Track im Fokus-Bereich bearbeiten", "Process this track in focused view"))
                                    .clicked()
                                {
                                    self.selected_segment_idx = Some(track.segment_index);
                                    self.selected_region = None;
                                    self.player.seek_seconds(track.start_seconds);
                                    self.console_tab = ConsoleTab::Filter;
                                }
                                let flt_tip = if self.language == Language::De {
                                    if track.is_filtered {
                                        format!("Rauschfilter für Track {:02} AKTIV (Klick zum Deaktivieren)", track.index)
                                    } else {
                                        format!("Rauschfilter für Track {:02} AUS (Klick zum Aktivieren)", track.index)
                                    }
                                } else if track.is_filtered {
                                    format!("Noise filter ACTIVE for Track {:02} (Click to disable)", track.index)
                                } else {
                                    format!("Noise filter OFF for Track {:02} (Click to enable)", track.index)
                                };
                                let btn_resp = bevel_button(
                                    ui,
                                    if track.is_filtered { "NR" } else { "NR–" },
                                    if track.is_filtered { BevelStyle::CompactEngaged } else { BevelStyle::Compact },
                                ).on_hover_text(flt_tip);
                                if btn_resp.clicked() {
                                    self.splitter.toggle_track_filter(track.segment_index);
                                }
                                btn_resp.context_menu(|ui| {
                                    if ui.add(action_button(self.text("Track bearbeiten …", "Process track …"))).clicked() {
                                        self.selected_segment_idx = Some(track.segment_index);
                                        self.selected_region = None;
                                        self.player.seek_seconds(track.start_seconds);
                                        self.console_tab = ConsoleTab::Filter;
                                        ui.close_menu();
                                    }
                                    let tog_txt = if track.is_filtered {
                                        self.text("Rauschfilter für diesen Track deaktivieren", "Disable noise filter for this track")
                                    } else {
                                        self.text("Rauschfilter für diesen Track aktivieren", "Enable noise filter for this track")
                                    };
                                    if ui.add(action_button(tog_txt)).clicked() {
                                        self.splitter.toggle_track_filter(track.segment_index);
                                        ui.close_menu();
                                    }
                                    if self.active_noise_profile.is_some() || self.denoiser_params.dc_filter_enabled {
                                        if ui.add(action_button(self.text("Filter jetzt in diesen Track einrechnen", "Bake filter into this track now"))).clicked() {
                                            self.bake_denoiser_into_track(ui.ctx(), track.segment_index);
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }

                            let pause_icon = if track.is_pause { "↩" } else { "||" };
                            let pause_tip = if track.is_pause {
                                self.language.btn_make_track()
                            } else {
                                self.language.btn_make_pause()
                            };
                            if bevel_button(ui, pause_icon, BevelStyle::Compact).on_hover_text(pause_tip).clicked() {
                                self.splitter.toggle_pause(index);
                            }

                            if index > 0 && bevel_button(ui, "×", BevelStyle::Compact)
                                .on_hover_text(self.text("Schnittmarke am Trackanfang entfernen", "Remove marker at track start"))
                                .clicked()
                            {
                                self.request_marker_delete(index - 1);
                            }
                        });
                    });
                ui.add_space(2.0);
            }
        });
    }

    fn update_hover_audition(
        &mut self,
        ui: &Ui,
        interaction: &crate::ui::waveform_widget::WaveformInteraction,
        has_audio: bool,
    ) {
        let hover = interaction.mouse_pos_in_waveform.filter(|_| {
            self.hover_preview_enabled && has_audio && !self.player.is_playing()
                && self.pending_delete_marker.is_none()
                && !ui.input(|input| input.pointer.any_down() || input.pointer.any_pressed())
        });
        let Some(pos) = hover else {
            self.clear_hover_audition();
            return;
        };
        if self.hover_mouse_pos.is_none_or(|previous| previous.distance(pos) > 6.0) {
            self.clear_hover_audition();
            self.hover_mouse_pos = Some(pos);
            self.hover_stationary_since = Some(std::time::Instant::now());
        }
        if let Some(since) = self.hover_stationary_since {
            let delay = Duration::from_millis(300);
            if since.elapsed() >= delay {
                if let Some(time) = interaction.hovered_time_sec {
                    if self.with_recording_output(|player| player.audition_at(time)) {
                        self.hover_audition_time = Some(time);
                    }
                    self.hover_stationary_since = None;
                    ui.ctx().request_repaint();
                }
            } else {
                ui.ctx().request_repaint_after(delay.saturating_sub(since.elapsed()));
            }
        } else if self.player.is_auditioning() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    fn clear_hover_audition(&mut self) {
        if self.hover_audition_time.take().is_some() {
            self.player.stop_audition();
        }
        self.hover_mouse_pos = None;
        self.hover_stationary_since = None;
    }

    fn render_export_panel(&mut self, ui: &mut Ui) {
        let p = palette(ui.ctx());
        let ctx = ui.ctx().clone();
        let progress = self.job.as_ref().and_then(|job| job.export_progress.clone());
        studio_panel(ui, self.text("WAV-Export", "WAV export"), |ui| {
            if let Some(progress) = progress {
                let (done, total) = progress.counts();
                let cancelling = progress.is_cancelled();
                ui.label(if cancelling {
                    self.text("Export wird abgebrochen …", "Cancelling export …")
                } else {
                    self.text("WAV-Dateien werden geschrieben …", "Writing WAV files …")
                });
                ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .text(format!("{done} / {total}")).animate(true));
                ui.add_space(12.0);
                if ui.add_enabled(!cancelling, action_button(self.language.btn_cancel())).clicked() {
                    progress.cancel();
                }
                ctx.request_repaint_after(Duration::from_millis(150));
                return;
            }

            if let Some(ExportOutcome::Done { count, folder }) = self.export_outcome.as_ref() {
                ui.label(RichText::new(format!("✓  {} {count}", self.text("Exportierte WAV-Dateien", "Exported WAV files")))
                    .strong().color(p.teal));
                let folder = folder.clone();
                let path = folder.display().to_string();
                ui.add(egui::Label::new(&path).truncate()).on_hover_text(path);
                ui.add_space(16.0);
                let done_label = self.text("Fertig", "Done");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(action_button(done_label)).clicked() {
                        self.export_outcome = None;
                    }
                    if ui.add(action_button(self.language.btn_show_in_finder())).clicked() {
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(folder).spawn();
                    }
                });
                return;
            }

            let mut tracks = self.splitter.get_tracks(self.player.total_seconds());
            if let Some(segment) = self.export_song {
                let title = tracks.iter().find(|track| track.segment_index == segment)
                    .map(|track| track.title.as_str()).unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("{} · {title}", self.text("Nur dieser Song", "This song only")));
                    if ui.add(compact_button(self.text("Alle Tracks anzeigen", "Show all tracks"))).clicked() {
                        self.export_song = None;
                        self.export_page = 0;
                    }
                });
                if self.export_song.is_some() { tracks.retain(|track| track.segment_index == segment); }
            }
            let target = self.effective_export_directory();
            let mut files = Vec::new();
            let artist = (!self.artist_name.trim().is_empty()).then_some(self.artist_name.trim());
            for track in tracks.iter().filter(|track| !track.is_pause) {
                for name in track.export_filenames_with_artist(artist) {
                    let exists = target.join(&name).exists();
                    files.push((track, name, exists));
                }
            }
            let count = files.len();
            let conflicts = files.iter().filter(|(_, _, exists)| *exists).count();
            let export_enabled = count > 0 && conflicts == 0 && self.current_audio_path.is_some()
                && self.job.is_none() && !self.recorder.is_busy();

            ui.horizontal_wrapped(|ui| {
                panel_title(ui, self.text("Zielordner", "Destination"));
                if ui.add(action_button(self.text("Basisordner ändern …", "Change base folder …"))).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().set_directory(&self.export_directory).pick_folder() {
                        self.export_directory = folder;
                        self.export_outcome = None;
                        self.save_config();
                        ctx.request_repaint();
                    }
                }
            });
            let target_label = self.effective_export_directory().display().to_string();
            ui.add(egui::Label::new(RichText::new(&target_label).color(p.text_secondary)).truncate())
                .on_hover_text(target_label);

            ui.add_space(8.0);
            panel_title(ui, self.text("Audio & Analog-Mastering", "Audio & Analog Mastering"));
            let format = if self.player.total_samples() > 0 {
                format!("WAV · PCM 24-bit · {} · {} Hz", self.text("Format je Song", "Per-song format"), self.player.sample_rate())
            } else {
                "WAV · PCM 24-bit".into()
            };
            ui.label(RichText::new(format).color(p.text_secondary).small());
            let mode_label = |mode: &NormalizationMode| match mode {
                NormalizationMode::None => self.text("Originalpegel", "Original level"),
                NormalizationMode::PeakPerTrack => self.text("Peak pro Track · −0,3 dBFS", "Peak per track · −0.3 dBFS"),
                NormalizationMode::PeakAlbum => self.text("Einheitlicher Pegel für das Album", "Shared gain across the album"),
            };
            let previous = self.normalization_mode.clone();
            let options = [NormalizationMode::None, NormalizationMode::PeakPerTrack, NormalizationMode::PeakAlbum];
            let labels = options.each_ref().map(mode_label);
            ui.horizontal_wrapped(|ui| {
                ui.weak(self.text("Normalisierung", "Normalization"));
                for (mode, label) in options.into_iter().zip(labels) {
                    if ui.add(mode_button(ui, label, self.normalization_mode == mode)).clicked() {
                        self.normalization_mode = mode;
                    }
                }
            });
            if self.normalization_mode != previous {
                self.save_config();
            }

            ui.horizontal_wrapped(|ui| {
                let hp_lbl = self.text("20 Hz Rumpel-/DC-Filter", "20 Hz subsonic filter");
                if ui.checkbox(&mut self.export_highpass_20hz, hp_lbl).changed() {
                    self.save_config();
                }
                ui.separator();
                ui.label(RichText::new(self.text("Schnitt-Fade:", "Cut fade:")).color(p.text_secondary).small());
                for (ms, lbl) in [(0_u32, self.text("Aus", "Off")), (15, "15 ms"), (100, "100 ms"), (300, "300 ms")] {
                    if ui.selectable_label(self.export_fade_ms == ms, lbl).clicked() {
                        self.export_fade_ms = ms;
                        self.save_config();
                    }
                }
                ui.separator();
                ui.label(RichText::new("L/R Trim:").color(p.text_secondary).small());
                ui.add(egui::DragValue::new(&mut self.export_balance_db).range(-12.0..=12.0).speed(0.1).suffix(" dB"));
                if self.active_noise_profile.is_some() {
                    ui.separator();
                    let nr_lbl = self.text("Rauschfilter anwenden", "Apply noise filter");
                    ui.checkbox(&mut self.export_apply_denoiser, nr_lbl);
                    if self.export_apply_denoiser {
                        let filtered_tracks_count = tracks.iter().filter(|t| !t.is_pause && t.is_filtered).count();
                        let total_active_tracks = tracks.iter().filter(|t| !t.is_pause).count();
                        ui.label(RichText::new(format!("({filtered_tracks_count}/{total_active_tracks} Tracks)")).color(p.text_secondary).small());
                        if ui.add(compact_button(self.text("Alle", "All"))).on_hover_text(self.text("Filter für alle Tracks aktivieren", "Enable filter for all tracks")).clicked() {
                            self.splitter.set_all_tracks_filtered(tracks.len(), true);
                        }
                        if ui.add(compact_button(self.text("Keine", "None"))).on_hover_text(self.text("Filter für alle Tracks deaktivieren", "Disable filter for all tracks")).clicked() {
                            self.splitter.set_all_tracks_filtered(tracks.len(), false);
                        }
                    }
                }
            });

            ui.add_space(8.0);
            let per_page = ((ui.available_height() - 152.0) / 33.0).floor().max(1.0) as usize;
            let pages = count.div_ceil(per_page).max(1);
            self.export_page = self.export_page.min(pages - 1);
            ui.horizontal(|ui| {
                panel_title(ui, &format!("{} {count}", self.text("Dateien", "Files")));
                page_controls(ui, &mut self.export_page, pages, self.language);
            });
            let gaps = tracks.iter().filter(|track| track.is_pause).count();
            if gaps > 0 {
                ui.label(RichText::new(format!("{} {gaps}", self.text("Ausgelassene Pausen", "Excluded gaps")))
                    .color(p.text_secondary).small());
            }
            if count == 0 {
                ui.label(self.text("Keine Tracks zum Exportieren.", "No tracks to export."));
            }
            for (track, name, exists) in files.iter().skip(self.export_page * per_page).take(per_page) {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.monospace(track.duration_formatted());
                    if track.processing.is_some() {
                        ui.colored_label(p.amber, self.text("Song-Bearbeitung", "Song processing"));
                    } else if self.export_apply_denoiser && self.active_noise_profile.is_some() {
                        let (flt_lbl, flt_col, flt_tip) = if track.is_filtered {
                            (
                                self.text("🔇 Filter", "🔇 Filter"),
                                p.teal,
                                self.text("Dieser Track wird entrauscht. Klick zum Deaktivieren.", "This track will be denoised. Click to disable."),
                            )
                        } else {
                            (
                                self.text("🔈 Bypass", "🔈 Bypass"),
                                p.text_muted,
                                self.text("Dieser Track wird ungefiltert exportiert. Klick zum Aktivieren.", "This track will be exported unfiltered. Click to enable."),
                            )
                        };
                        if ui.add(compact_button(RichText::new(flt_lbl).color(flt_col).small())).on_hover_text(flt_tip).clicked() {
                            self.splitter.toggle_track_filter(track.segment_index);
                        }
                    }
                    if *exists {
                        ui.colored_label(p.amber, self.text("Existiert bereits", "Already exists"));
                    }
                    ui.add(egui::Label::new(name).truncate()).on_hover_text(name);
                });
            }
            if conflicts > 0 {
                ui.colored_label(p.amber, self.text(
                    "Wähle einen anderen Basisordner oder räume den Zielordner frei.",
                    "Choose a different base folder or clear the conflicting files.",
                ));
            }
            match &self.export_outcome {
                Some(ExportOutcome::Failed(error)) => { ui.add(egui::Label::new(RichText::new(error).color(p.red)).truncate()).on_hover_text(error); }
                Some(ExportOutcome::Cancelled) => {
                    ui.label(self.text("Export abgebrochen.", "Export cancelled."));
                }
                _ => {}
            }
            ui.add_space(8.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = self.text("Exportieren", "Export");
                let export = primary_button(ui, label);
                if ui.add_enabled(export_enabled, export).clicked() && self.effective_export_directory() == target {
                    if let Some(source) = self.current_audio_path.clone() {
                        self.export_outcome = None;
                        let denoiser = if self.export_apply_denoiser && self.active_noise_profile.is_some() {
                            let mut p = self.denoiser_params.clone();
                            p.enabled = true;
                            p.audition_mode = AuditionMode::Wet;
                            Some((self.active_noise_profile.clone(), p))
                        } else {
                            None
                        };
                        let opts = ExportOptions {
                            norm_mode: self.normalization_mode.clone(),
                            highpass_20hz: self.export_highpass_20hz,
                            fade_ms: self.export_fade_ms,
                            stereo_balance_db: self.export_balance_db,
                            artist: (!self.artist_name.trim().is_empty()).then(|| self.artist_name.trim().to_string()),
                            denoiser,
                        };
                        let progress = Arc::new(ExportProgress::default());
                        let worker_progress = progress.clone();
                        self.start_job(&ctx, self.text("Tracks werden exportiert …", "Exporting tracks …"), move || {
                            let files = export_tracks_with_options(&source, &target, &tracks, &opts, |done, total| {
                                worker_progress.report(done, total)
                            })?;
                            Ok(JobResult::Export(files.len(), target))
                        });
                        if let Some(job) = &mut self.job {
                            job.export_progress = Some(progress);
                        }
                    }
                }
            });
        });
    }

}

fn page_controls(ui: &mut Ui, page: &mut usize, pages: usize, language: Language) {
    if pages <= 1 {
        return;
    }
    let (next, previous) = match language {
        Language::De => ("Nächste Seite", "Vorherige Seite"),
        Language::En => ("Next page", "Previous page"),
    };
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add_enabled(*page + 1 < pages, action_button("›")).on_hover_text(next).clicked() {
            *page += 1;
        }
        ui.label(RichText::new(format!("{} / {pages}", *page + 1)).monospace());
        if ui.add_enabled(*page > 0, action_button("‹")).on_hover_text(previous).clicked() {
            *page -= 1;
        }
    });
}

fn titles_without_padding(titles: &[String]) -> &[String] {
    let end = titles.iter().rposition(|title| !title.is_empty()).map_or(0, |index| index + 1);
    &titles[..end]
}

fn dirs_default_music_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let music = PathBuf::from(home).join("Music").join("TapeDigizer");
        Some(music)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splitter::export_tracks;

    #[test]
    #[ignore = "writes up to 1 GiB of temporary export data per case"]
    fn profile_long_session() {
        use std::io::{Seek, SeekFrom, Write};
        use std::time::Instant;

        for (minutes, rate) in [(45_u32, 44_100_u32), (60, 48_000)] {
            let frames = minutes * 60 * rate;
            let data_bytes = frames * 6;
            let (source, mut file) = crate::project::unique_file(
                &std::env::temp_dir(), "tapedigizer-profile", "wav",
            ).unwrap();
            let output = source.with_extension("export");
            file.write_all(b"RIFF").unwrap();
            file.write_all(&(36 + data_bytes).to_le_bytes()).unwrap();
            file.write_all(b"WAVEfmt ").unwrap();
            file.write_all(&16_u32.to_le_bytes()).unwrap();
            file.write_all(&1_u16.to_le_bytes()).unwrap(); // PCM
            file.write_all(&2_u16.to_le_bytes()).unwrap(); // stereo
            file.write_all(&rate.to_le_bytes()).unwrap();
            file.write_all(&(rate * 6).to_le_bytes()).unwrap();
            file.write_all(&6_u16.to_le_bytes()).unwrap();
            file.write_all(&24_u16.to_le_bytes()).unwrap();
            file.write_all(b"data").unwrap();
            file.write_all(&data_bytes.to_le_bytes()).unwrap();
            file.set_len(u64::from(data_bytes) + 44).unwrap();
            // Sparse synthetic audio, with one impulse per minute.
            for minute in 0..minutes {
                file.seek(SeekFrom::Start(44 + u64::from(minute * 60 * rate) * 6)).unwrap();
                file.write_all(&[0, 0, 64, 0, 0, 192]).unwrap();
            }
            drop(file);

            let opened = Instant::now();
            let audio = AudioPlayer::decode_file(&source).unwrap();
            let waveform = WaveformCache::build(&audio);
            analyze_samples(&audio.left, &audio.right, rate).unwrap();
            let open_time = opened.elapsed();
            let ctx = egui::Context::default();
            let duration = f64::from(minutes * 60);
            let mut frame_ms = Vec::new();
            for i in 0..121 {
                let start = Instant::now();
                let mut zoom = 1.0 + (i % 24) as f32;
                let mut vertical_zoom = 1.0;
                let mut scroll = 0.35;
                let _ = ctx.run(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1140.0, 840.0))),
                    ..Default::default()
                }, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_stereo_waveform(
                            ui, waveform.for_view(f64::from(zoom)), duration, 0.0, false,
                            &[duration / 3.0, duration / 2.0], &[], &[], &mut None, None,
                            &mut zoom, &mut vertical_zoom, 280.0, &mut scroll, EditTool::Pointer, false, None, false,
                        );
                    });
                });
                if i > 0 {
                    frame_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            frame_ms.sort_by(f64::total_cmp);
            let p95 = frame_ms[113];
            let exporter = Instant::now();
            let files = export_tracks(
                &source, &output, &TrackSplitter::new().get_tracks(duration),
                NormalizationMode::None, |_, _| true,
            ).unwrap();
            let export_time = exporter.elapsed();
            assert_eq!(hound::WavReader::open(&files[0]).unwrap().duration(), frames);
            eprintln!("{minutes} min / {rate} Hz: prepare {open_time:.3?}, zoom CPU p95 {p95:.2} ms, export {export_time:.3?}");
            drop(audio);
            std::fs::remove_dir_all(output).unwrap();
            std::fs::remove_file(source).unwrap();
        }
    }

    fn test_app() -> TapeDigizerApp {
        TapeDigizerApp {
            language: Language::De,
            tape_name: "Test Tape".into(),
            artist_name: String::new(),
            media_source: MediaSourceType::Cassette,
            tape_side: TapeSide::SideA,
            current_project_path: None,
            current_audio_path: None,
            pending_delete_marker: None,
            console_tab: ConsoleTab::Input,
            available_devices: Vec::new(),
            selected_device_idx: 0,
            channel_pairs: Vec::new(),
            selected_pair_idx: 0,
            recorder: AudioRecorder::new(),
            player: AudioPlayer::new(),
            song_editor: SongEditor::default(),
            splitter: TrackSplitter::new(),
            selected_timer_minutes: 45,
            time_headroom_minutes: 5,
            enable_silence_autostop: true,
            autostop_threshold_db: -48.0,
            autostop_silence_secs: 10.0,
            run_on_seconds: 15,
            pause_threshold_db: -45.0,
            pause_min_seconds: 1.8,
            pause_as_gaps: false,
            zero_crossing_snap: true,
            analysis_report: None,
            waveform_cache: WaveformCache::default(),
            selected_marker_idx: None,
            marker_drag: None,
            marker_drag_cancelled: false,
            transient_cache: None,
            selected_segment_idx: None,
            waveform_zoom: 1.0,
            waveform_vertical_zoom: 1.0,
            track_page: 0,
            export_page: 0,
            export_song: None,
            selected_region: None,
            scope_radius_sec: 1.5,
            scope_locked_center: None,
            transient_sensitivity: 1.0,
            show_transients: true,
            scope_zoom: 1.0,
            scope_gain_db: 0.0,
            scope_auto_gain: true,
            scope_scroll: 0.0,
            waveform_scroll: 0.0,
            active_edit_tool: EditTool::Pointer,
            hover_preview_enabled: true,
            hover_mouse_pos: None,
            hover_stationary_since: None,
            hover_audition_time: None,
            export_directory: PathBuf::from("/tmp"),
            normalization_mode: NormalizationMode::None,
            export_highpass_20hz: false,
            export_fade_ms: 15,
            export_balance_db: 0.0,
            export_outcome: None,
            denoiser_params: DenoiserParams::default(),
            active_noise_profile: None,
            active_noise_samples: None,
            saved_noise_presets: Vec::new(),
            selected_preset_idx: None,
            preset_save_name: String::new(),
            audio_status: None,
            export_apply_denoiser: true,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            batch_titles_open: false,
            batch_titles_text: String::new(),
            #[cfg(target_os = "macos")]
            mac_menu: None,
        }
    }

    fn await_job(app: &mut TapeDigizerApp) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while app.job.is_some() && std::time::Instant::now() < deadline {
            app.poll_job();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.job.is_none(), "background task timed out");
    }

    #[test]
    fn audio_history_restores_the_matching_file_and_markers_for_save_and_export() {
        use crate::audio::denoiser::write_stereo_wav_atomic;

        let (sentinel, _) = crate::project::unique_file(&std::env::temp_dir(), "tapedigizer-history", "tmp").unwrap();
        let dir = sentinel.with_extension("");
        std::fs::create_dir(&dir).unwrap();
        std::fs::remove_file(sentinel).unwrap();
        let source = dir.join("original.wav");
        let project = dir.join("saved").join("project.tapedigizer");
        let ctx = egui::Context::default();
        let mut app = test_app();
        let tone = vec![0.125; 1000];
        write_stereo_wav_atomic(&source, &tone, &tone, 1000).unwrap();
        let original_bytes = std::fs::read(&source).unwrap();

        app.install_recording_revision(PreparedAudio::read(source.clone()).unwrap());
        app.splitter.audio_revision = Some(source.clone());
        app.splitter.add_marker(0.5, 1.0, 1000);
        app.song_editor.view.selected_segment = Some(1);
        app.song_editor.view.loop_playback = true;
        app.save_project_to_path(&ctx, &project);
        await_job(&mut app);
        assert!(app.ui_error_message.is_none());
        assert!(!app.has_unsaved_edits());
        let project_audio = app.current_audio_path.clone().unwrap();
        assert_ne!(project_audio, source, "Save As owns a copy in the project folder");
        assert_eq!(std::fs::read(&project_audio).unwrap(), original_bytes);
        assert_eq!(app.splitter.history_target(false).unwrap().audio_revision.as_deref(), Some(project_audio.as_path()));
        app.selected_region = Some((0.2, 0.8));
        app.cut_or_trim_recording(&ctx, true);
        await_job(&mut app);
        assert!(app.ui_error_message.is_none());
        let edited = app.current_audio_path.clone().unwrap();
        assert_ne!(edited, source);
        assert_eq!(app.player.total_samples(), 600);
        assert_eq!(app.splitter.markers, [0.3]);
        assert!(app.has_unsaved_edits());
        let (previous_document, previous_audio) = TapeDigizerProject::load_from_file(&project).unwrap();
        assert_eq!(previous_document.markers, [0.5]);
        assert_eq!(previous_audio.as_deref(), Some(project_audio.as_path()));

        app.undo_marker_edit(&ctx, false);
        await_job(&mut app);
        assert_eq!(app.current_audio_path.as_deref(), Some(project_audio.as_path()));
        assert_eq!(app.player.total_samples(), 1000);
        assert_eq!(app.splitter.markers, [0.5]);
        assert!(!app.has_unsaved_edits());
        assert!(app.song_editor.view.loop_playback);
        assert_eq!(app.song_editor.view.selected_segment, Some(1));

        let moved = edited.with_extension("test-missing");
        std::fs::rename(&edited, &moved).unwrap();
        app.undo_marker_edit(&ctx, true);
        await_job(&mut app);
        assert!(app.ui_error_message.is_some());
        assert_eq!(app.current_audio_path.as_deref(), Some(project_audio.as_path()));
        assert!(app.splitter.can_redo());
        std::fs::rename(moved, &edited).unwrap();
        app.undo_marker_edit(&ctx, true);
        await_job(&mut app);
        assert!(app.ui_error_message.is_none());
        assert_eq!(app.current_audio_path.as_deref(), Some(edited.as_path()));
        assert_eq!(app.splitter.markers, [0.3]);

        app.save_project_to_path(&ctx, &project);
        await_job(&mut app);
        assert!(app.ui_error_message.is_none());
        let (saved, wave) = TapeDigizerProject::load_from_file(&project).unwrap();
        assert_eq!(saved.markers, [0.3]);
        assert_eq!(wave.as_deref(), Some(edited.as_path()));
        let files = export_tracks(
            wave.as_deref().unwrap(),
            &dir.join("export"),
            &app.splitter.get_tracks(app.player.total_seconds()),
            NormalizationMode::None,
            |_, _| true,
        ).unwrap();
        let total_exported_frames: u32 = files.iter().map(|path| hound::WavReader::open(path).unwrap().duration()).sum();
        assert_eq!(total_exported_frames, 600);
        app.denoiser_params.enabled = true;
        app.denoiser_params.dc_filter_enabled = true;
        app.bake_denoiser_into_recording(&ctx);
        await_job(&mut app);
        assert!(!app.denoiser_params.enabled);
        assert_eq!(app.splitter.disabled_filter_indices, [0, 1]);
        app.undo_marker_edit(&ctx, false);
        await_job(&mut app);
        assert!(app.denoiser_params.enabled, "Undo restores the original revision's live preview");
        assert!(app.splitter.disabled_filter_indices.is_empty());
        app.undo_marker_edit(&ctx, true);
        await_job(&mut app);
        assert!(!app.denoiser_params.enabled, "Redo monitors the rendered revision without filtering it twice");
        assert_eq!(std::fs::read(source).unwrap(), original_bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn console_renders_before_and_after_recording() {
        let ctx = egui::Context::default();
        apply_theme(&ctx, ThemeMode::Silverface);
        let mut app = test_app();

        let screen = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1060.0, 700.0));
        for tab in [ConsoleTab::Input, ConsoleTab::Edit, ConsoleTab::Tracks, ConsoleTab::Filter, ConsoleTab::Export] {
            app.console_tab = tab;
            let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ctx| {
                app.render_console_header(ctx);
                app.render_console(ctx);
            });
        }

        // A one-track project with a short name in a compact single-line row must still get a wide input (>= 20 chars).
        app.splitter.custom_titles = vec!["cghcg".into()];
        let tracks = app.splitter.get_tracks(1800.0);
        for (window_width, minimum_field_width) in [(800.0, 520.0), (1140.0, 860.0)] {
            // The scroll area uses the previous frame to calculate its clip rectangle.
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(window_width, 540.0))),
                    ..Default::default()
                }, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().inner_margin(16.0))
                        .show(ctx, |ui| {
                            app.render_track_list(ui, &tracks);
                        });
                });
            }
            let field = ctx.read_response(egui::Id::new("track_title_field").with(0_usize)).unwrap();
            assert!(field.interact_rect.width() >= minimum_field_width,
                "Track title is only {} pt wide at {window_width} pt", field.interact_rect.width());
        }

        let audio = DecodedAudio {
            left: vec![0.1; 120_000],
            right: vec![0.2; 120_000],
            sample_rate: 1000,
        };
        let waveform = WaveformCache::build(&audio);
        let report = analyze_samples(&audio.left, &audio.right, audio.sample_rate).unwrap();
        app.job = Some(PendingJob::completed("Analyzing recording", JobResult::Recording(audio, waveform, report)));
        app.poll_job();
        // Playback device availability is independent of the console layout.
        app.player.load_test_samples(DecodedAudio {
            left: vec![0.1; 120_000],
            right: vec![0.2; 120_000],
            sample_rate: 1000,
        });
        assert!(app.console_tab == ConsoleTab::Edit, "Recording completion must open the edit tab");
        app.splitter.markers = (1..10).map(|n| 10.0 * f64::from(n)).collect();
        app.selected_marker_idx = None;
        app.selected_segment_idx = Some(0);
        app.batch_titles_open = true;
        app.scope_auto_gain = false;
        app.pause_preview = Some(app.splitter.snapshot());
        app.current_audio_path = Some(PathBuf::from("/tmp/recording.wav"));
        app.learn_noise_from_region(0.0, 2.0);
        app.splitter.custom_titles[0] = "Ein langer Titel für den Layouttest der TapeDigizer-Konsole".repeat(3);
        for theme in [ThemeMode::Silverface, ThemeMode::Obsidian] {
            apply_theme(&ctx, theme);
            for language in [Language::De, Language::En] {
                app.language = language;
                for tab in [ConsoleTab::Input, ConsoleTab::Edit, ConsoleTab::Tracks, ConsoleTab::Filter, ConsoleTab::Export] {
                    app.console_tab = tab;
                    for size in [screen, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1260.0, 820.0))] {
                        let variants = match tab { ConsoleTab::Filter => 7, ConsoleTab::Edit => 2, _ => 1 };
                        for variant in 0..variants {
                            if tab == ConsoleTab::Filter {
                                app.song_editor.select_layout_test_module(variant);
                            }
                            let _ = ctx.run(egui::RawInput { screen_rect: Some(size), ..Default::default() }, |ctx| {
                                if tab == ConsoleTab::Edit {
                                    ctx.data_mut(|data| data.insert_temp(egui::Id::new("whole_recording_processing"), variant == 1));
                                }
                                app.render_console_header(ctx);
                                app.render_console(ctx);
                            });
                        }
                    }
                }
            }
        }
        assert_eq!(app.selected_segment_idx, Some(0), "Rendering the scope must keep the selected first track");
        assert!(app.selected_marker_idx.is_none());
        assert!(ctx.memory(|memory| memory.area_rect(egui::Id::new("export_dialog"))).is_none());
    }
}

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use egui::{Color32, Pos2, RichText, Stroke, Ui, Vec2};
use crate::analysis::{analyze_samples, AudioAnalysisReport, LevelDiagnosis};
use crate::audio::denoiser::{
    crop_trim_audio_to_range, cut_delete_audio_range, write_stereo_wav_atomic,
    AuditionMode, DenoiserParams, NoisePresetManager, NoiseProfile, SpectralDenoiser,
};
use crate::audio::device::{get_channel_pairs_for_device, list_input_devices, AudioDeviceInfo, ChannelPair};
use crate::audio::player::{AudioPlayer, DecodedAudio};
use crate::audio::recorder::{AudioRecorder, RecorderState, StereoWaveformPoint};
use crate::splitter::{
    export_tracks_with_options, ExportOptions, MarkerSnapshot, NormalizationMode, TrackSplitter, TransientPoint,
};
use crate::ui::knob_widget::render_gain_knob_card;
use crate::ui::meter_widget::render_stereo_meter;
use crate::ui::theme::{
    draw_recessed_well, mode_button, panel_frame, panel_title, primary_button, record_button,
    studio_panel, studio_wordmark, time_display, ACCENT, BG_INSET, BG_PANEL, BG_WINDOW,
    BORDER_MID, LED_AMBER, LED_GREEN, LED_RED, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};
use crate::i18n::Language;
use crate::project::{sanitize_filename, MediaSourceType, TapeDigizerProject, TapeSide};
use crate::ui::waveform_widget::{render_stereo_waveform, EditTool};

#[derive(PartialEq, Clone, Copy)]
pub enum ActiveTab {
    Recording,
    Diagnosis,
    SplitAndExport,
}

enum JobResult {
    Open(OpenedProject),
    Recording(DecodedAudio, WaveformCache, AudioAnalysisReport),
    Export(usize, PathBuf),
    Pauses(MarkerSnapshot),
    Saved(PathBuf, Option<PathBuf>, SavedEdits),
    AudioModified {
        audio: DecodedAudio,
        waveform: WaveformCache,
        report: AudioAnalysisReport,
        markers: MarkerSnapshot,
        status_msg: Option<String>,
        disable_live_denoiser: bool,
    },
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

struct PreparedAudio {
    path: PathBuf,
    decoded: DecodedAudio,
    waveform: WaveformCache,
    report: AudioAnalysisReport,
}

#[derive(Default)]
struct ExportProgress {
    cancelled: AtomicBool,
    finished: AtomicUsize,
    total: AtomicUsize,
}

impl ExportProgress {
    fn report(&self, finished: usize, total: usize) -> bool {
        self.total.store(total, Ordering::Relaxed);
        self.finished.store(finished, Ordering::Relaxed);
        !self.cancelled.load(Ordering::Relaxed)
    }
}

struct PendingJob {
    label: &'static str,
    result: std::sync::mpsc::Receiver<Result<JobResult, String>>,
    export_progress: Option<Arc<ExportProgress>>,
}

#[derive(PartialEq)]
struct SavedEdits {
    name: String,
    side: TapeSide,
    markers: MarkerSnapshot,
    normalization: NormalizationMode,
}

struct TransientCache {
    marker_time: f64,
    radius: f64,
    sensitivity: f32,
    points: std::sync::Arc<[TransientPoint]>,
}

pub struct TapeDigizerApp {
    // Internationalization
    pub language: Language,

    // Tape & Album Organization
    pub media_source: MediaSourceType,
    pub tape_name: String,
    pub artist_name: String,
    pub tape_side: TapeSide,
    pub current_project_path: Option<PathBuf>,
    pub current_audio_path: Option<PathBuf>,

    // Marker Delete Confirmation
    pub pending_delete_marker: Option<(usize, f64, String)>,

    // Current Navigation Tab
    active_tab: ActiveTab,

    // Audio Devices & Configuration
    available_devices: Vec<AudioDeviceInfo>,
    selected_device_idx: usize,
    channel_pairs: Vec<ChannelPair>,
    selected_pair_idx: usize,

    // Audio Engines
    recorder: AudioRecorder,
    player: AudioPlayer,
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
    magnifier_scope_active: bool,
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
    denoiser_panel_active: bool,
    denoiser_params: DenoiserParams,
    active_noise_profile: Option<NoiseProfile>,
    active_noise_samples: Option<(Vec<f32>, Vec<f32>, u32)>,
    saved_noise_presets: Vec<NoiseProfile>,
    selected_preset_idx: Option<usize>,
    preset_save_name: String,
    auto_overwrite_original_wav: bool,
    last_saved_bytes_info: Option<String>,
    export_apply_denoiser: bool,

    // General status/error
    ui_error_message: Option<String>,
    job: Option<PendingJob>,
    saved_edits: Option<SavedEdits>,
    pause_preview: Option<MarkerSnapshot>,
    show_tracks: bool,
    show_export: bool,
    #[cfg(target_os = "macos")]
    mac_menu: Option<super::macos_menu::MacMenu>,
}

impl TapeDigizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(crate::ui::theme::studio_visuals());
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 6.0);
            style.spacing.button_padding = Vec2::new(8.0, 4.0);
        });

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
            active_tab: ActiveTab::Recording,
            available_devices: devices,
            selected_device_idx: selected_dev_idx,
            channel_pairs,
            selected_pair_idx,
            recorder,
            player: AudioPlayer::new(),
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
            magnifier_scope_active: false,
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
            denoiser_panel_active: false,
            denoiser_params: DenoiserParams::default(),
            active_noise_profile: None,
            active_noise_samples: None,
            saved_noise_presets: NoisePresetManager::list_presets(),
            selected_preset_idx: None,
            preset_save_name: String::new(),
            auto_overwrite_original_wav: true,
            last_saved_bytes_info: None,
            export_apply_denoiser: true,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            show_tracks: true,
            show_export: false,
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
        self.selected_marker_idx = None;
        self.selected_segment_idx = None;
        self.pending_delete_marker = None;
        self.marker_drag = None;
        self.selected_region = None;
        self.scope_locked_center = None;
        self.pause_preview = None;
        self.show_export = false;
        self.batch_titles_open = false;
        self.export_outcome = None;
        self.transient_cache = None;
        self.magnifier_scope_active = false;
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
        let (sender, result) = std::sync::mpsc::sync_channel(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(work());
            ctx.request_repaint();
        });
        self.job = Some(PendingJob { label, result, export_progress: None });
        self.ui_error_message = None;
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
            if self.show_export && !matches!(command, Command::Quit) {
                continue;
            }
            match command {
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Command::New => self.new_project(),
                Command::Open => self.open_project_dialog(ctx),
                Command::Save => self.save_project_quick(ctx),
                Command::SaveAs => self.save_project_dialog(ctx),
                Command::Reveal => self.reveal_session_in_finder(),
                Command::Undo | Command::Redo if self.job.is_none() => {
                    let redo = matches!(command, Command::Redo);
                    if ctx.wants_keyboard_input() {
                        let modifiers = egui::Modifiers { command: true, mac_cmd: true, shift: redo, ..Default::default() };
                        ctx.input_mut(|input| input.events.push(egui::Event::Key {
                            key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers,
                        }));
                    } else {
                        self.undo_marker_edit(redo);
                    }
                }
                Command::Record => self.active_tab = ActiveTab::Recording,
                Command::Levels if !self.recorder.is_busy() => self.active_tab = ActiveTab::Diagnosis,
                Command::Edit if !self.recorder.is_busy() => self.active_tab = ActiveTab::SplitAndExport,
                Command::Export if !self.recorder.is_busy() && self.job.is_none() && self.player.total_samples() > 0 => {
                    self.active_tab = ActiveTab::SplitAndExport;
                    self.export_page = 0;
                    self.show_export = true;
                }
                Command::ToggleFilterStudio => {
                    self.denoiser_panel_active = !self.denoiser_panel_active;
                    if self.denoiser_panel_active {
                        self.magnifier_scope_active = false;
                        self.refresh_noise_presets();
                        self.active_tab = ActiveTab::SplitAndExport;
                    }
                }
                Command::ToggleScope => {
                    self.magnifier_scope_active = !self.magnifier_scope_active;
                    if self.magnifier_scope_active {
                        self.denoiser_panel_active = false;
                        self.active_tab = ActiveTab::SplitAndExport;
                    }
                }
                Command::ToggleTracks => {
                    self.show_tracks = !self.show_tracks;
                }
                Command::ToggleLiveFilter => {
                    self.denoiser_params.enabled = !self.denoiser_params.enabled;
                    self.player.set_denoiser_params(self.denoiser_params.clone());
                }
                Command::LearnNoise if self.player.total_samples() > 0 => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let (s0, s1) = self.active_selection_or_window(total_sec, cur_sec);
                    self.learn_noise_from_region(s0, s1);
                }
                Command::BakeFilterTrack if self.player.total_samples() > 0 => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let tracks = self.splitter.get_tracks(total_sec);
                    let seg_idx = self.selected_segment_idx.unwrap_or_else(|| {
                        tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds)
                            .map(|t| t.segment_index).unwrap_or(0)
                    });
                    self.bake_denoiser_into_track(ctx, seg_idx);
                }
                Command::BakeFilterAll if self.player.total_samples() > 0 => {
                    self.bake_denoiser_into_recording(ctx);
                }
                Command::FilterAllTracks => {
                    let total_sec = self.player.total_seconds();
                    let count = self.splitter.get_tracks(total_sec).len();
                    self.splitter.set_all_tracks_filtered(count, true);
                }
                Command::FilterNoTracks => {
                    let total_sec = self.player.total_seconds();
                    let count = self.splitter.get_tracks(total_sec).len();
                    self.splitter.set_all_tracks_filtered(count, false);
                }
                Command::ToggleSelectedTrackFilter => {
                    let total_sec = self.player.total_seconds();
                    let cur_sec = self.player.current_seconds();
                    let tracks = self.splitter.get_tracks(total_sec);
                    let seg_idx = self.selected_segment_idx.unwrap_or_else(|| {
                        tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds)
                            .map(|t| t.segment_index).unwrap_or(0)
                    });
                    self.splitter.toggle_track_filter(seg_idx);
                }
                Command::ToggleSubsonicFilter => {
                    self.export_highpass_20hz = !self.export_highpass_20hz;
                    self.save_config();
                }
                _ => {}
            }
        }
    }

    fn undo_marker_edit(&mut self, redo: bool) {
        if self.job.is_some() || self.marker_drag.is_some() || self.recorder.is_busy() { return; }
        let changed = if redo { self.splitter.redo() } else { self.splitter.undo() };
        if changed {
            self.selected_marker_idx = None;
            self.selected_segment_idx = None;
            self.pending_delete_marker = None;
        }
    }

    fn poll_job(&mut self) {
        use std::sync::mpsc::TryRecvError;
        let Some(job) = &self.job else { return; };
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(self.text(
                "Hintergrundaufgabe wurde unterbrochen.", "Background job stopped unexpectedly.",
            ).into()),
        };
        let export = job.export_progress.is_some();
        self.job = None;
        match result {
            Ok(JobResult::Open(opened)) => {
                let project = opened.document;
                if let Some(audio) = opened.audio {
                    if let Err(e) = self.player.load_decoded(audio.decoded) {
                        self.ui_error_message = Some(e);
                        return;
                    }
                    self.current_audio_path = Some(audio.path);
                    self.waveform_cache = audio.waveform;
                    self.analysis_report = Some(audio.report);
                    self.active_tab = ActiveTab::SplitAndExport;
                } else {
                    self.player.reset();
                    self.current_audio_path = None;
                    self.waveform_cache = WaveformCache::default();
                    self.analysis_report = None;
                    self.active_tab = ActiveTab::Recording;
                }
                self.tape_name = project.tape_name;
                self.tape_side = project.tape_side;
                self.splitter = TrackSplitter::new();
                self.splitter.restore(MarkerSnapshot {
                    markers: project.markers,
                    custom_titles: project.custom_titles,
                    pause_indices: project.pause_indices,
                    disabled_filter_indices: project.disabled_filter_indices,
                });
                self.normalization_mode = match project.normalization_mode.as_str() {
                    "track" => NormalizationMode::PeakPerTrack,
                    "album" => NormalizationMode::PeakAlbum,
                    _ => NormalizationMode::None,
                };
                self.current_project_path = opened.project_path;
                self.reset_editor_selection();
                self.saved_edits = self.current_project_path.as_ref().map(|_| self.edits());

            }
            Ok(JobResult::Recording(audio, waveform, report)) => {
                self.waveform_cache = waveform;
                self.analysis_report = Some(report);
                self.active_tab = ActiveTab::Diagnosis;
                if let Err(e) = self.player.load_decoded(audio) {
                    self.ui_error_message = Some(e);
                }
            }
            Ok(JobResult::Export(count, folder)) => {
                self.export_outcome = Some(ExportOutcome::Done { count, folder });
            }
            Ok(JobResult::Pauses(snapshot)) => self.pause_preview = Some(snapshot),
            Ok(JobResult::Saved(path, audio_path, edits)) => {
                self.apply_saved_project(path, audio_path, edits);
            }
            Ok(JobResult::AudioModified {
                audio,
                waveform,
                report,
                markers,
                status_msg,
                disable_live_denoiser,
            }) => {
                self.waveform_cache = waveform;
                self.analysis_report = Some(report);
                let before = self.splitter.snapshot();
                self.splitter.restore(markers);
                self.splitter.commit_edit(before);
                self.selected_region = None;
                self.selected_marker_idx = None;
                self.selected_segment_idx = None;
                self.last_saved_bytes_info = status_msg;
                if disable_live_denoiser {
                    self.denoiser_params.enabled = false;
                    self.player.set_denoiser_params(self.denoiser_params.clone());
                }
                if let Err(e) = self.player.replace_decoded(audio) {
                    self.ui_error_message = Some(e);
                }
                // Keep associated .tapedigizer project file in sync with modified audio
                if let Some(proj_path) = self.current_project_path.clone() {
                    let doc = self.project_document();
                    let src = self.current_audio_path.clone();
                    if doc.save_to_file(&proj_path, src.as_deref()).is_ok() {
                        self.saved_edits = Some(self.edits());
                    }
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
        while markers.custom_titles.last().is_some_and(String::is_empty) {
            markers.custom_titles.pop();
        }
        SavedEdits { name: self.tape_name.clone(), side: self.tape_side, markers, normalization: self.normalization_mode.clone() }
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
            || self.saved_edits.as_ref() == Some(&self.edits())
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
        TapeDigizerProject::new(
            self.tape_name.clone(),
            self.tape_side,
            String::new(),
            self.splitter.markers.clone(),
            self.splitter.custom_titles.clone(),
            self.splitter.pause_indices.clone(),
            self.splitter.disabled_filter_indices.clone(),
            norm.into(),
        )
    }

    fn apply_saved_project(&mut self, path: PathBuf, audio: Option<PathBuf>, edits: SavedEdits) {
        self.ui_error_message = None;
        if let Some(audio) = audio {
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
        self.active_tab = ActiveTab::Recording;
        self.ui_error_message = None;

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
                    let decoded = AudioPlayer::decode_file(&path)?;
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
                    let waveform = WaveformCache::build(&decoded);
                    let report = analyze_samples(&decoded.left, &decoded.right, decoded.sample_rate)?;
                    Some(PreparedAudio { path, decoded, waveform, report })
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
        self.ui_error_message = save_error;
    }

    fn preview_pauses(&mut self, ctx: &egui::Context, gaps: bool) {
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let rate = self.player.sample_rate();
        let thresh_db = self.pause_threshold_db;
        let min_sec = f64::from(self.pause_min_seconds);
        self.start_job(ctx, self.text("Pausen werden gesucht …", "Finding pauses …"), move || {
            let mut proposal = TrackSplitter::new();
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
        let recorder_state = self.recorder.state();
        let waiting_for_signal = recorder_state == RecorderState::WaitingForSignal;
        let run_on = recorder_state == RecorderState::RunOn;
        let is_recording = matches!(recorder_state, RecorderState::Recording | RecorderState::RunOn);
        let finalizing = recorder_state == RecorderState::Finalizing;
        let is_monitoring = self.recorder.state() == RecorderState::Monitoring;
        let is_playing = self.player.is_playing();

        if self.recorder.is_busy() || is_monitoring || is_playing {
            ctx.request_repaint();
        }

        self.recorder.poll_auto_stop();
        if self.recorder.poll_finished() {
            self.finish_recording_and_analyze(ctx);
        }

        // Global Command Shortcuts - read all keys in a single lock-free query
        let (cmd, shift, key_n, key_s, key_o, key_z, key_y, key_space, key_m, key_h, key_l, key_c, key_v, key_r, key_esc, key_del, key_left, key_right, key_1, key_2, key_3) = ctx.input(|i| (
            i.modifiers.command,
            i.modifiers.shift,
            i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::O),
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
            i.key_pressed(egui::Key::Num2),
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

        if !self.show_export {
            if cmd && key_1 {
                self.active_tab = ActiveTab::Recording;
            }
            if cmd && key_2 && !self.recorder.is_busy() {
                self.active_tab = ActiveTab::Diagnosis;
            }
            if cmd && key_3 && !self.recorder.is_busy() {
                self.active_tab = ActiveTab::SplitAndExport;
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

            if !ctx.wants_keyboard_input() {
                if cmd && key_z {
                    if shift {
                        self.undo_marker_edit(true);
                    } else {
                        self.undo_marker_edit(false);
                    }
                } else if cmd && key_y {
                    self.undo_marker_edit(true);
                }
            }

            // Global Shortcuts in Tab 3 (Split & Export)
            let has_audio = self.player.total_seconds() > 0.1;
            if self.active_tab == ActiveTab::SplitAndExport && !ctx.wants_keyboard_input() && self.job.is_none() && !self.recorder.is_busy() {
                if self.pending_delete_marker.is_some() && key_esc {
                    self.pending_delete_marker = None;
                }

                if has_audio && key_space {
                    self.player.toggle_play();
                }
                if has_audio && key_m {
                    let cur_sec = self.player.current_seconds();
                    self.add_marker(cur_sec);
                }
                if key_h {
                    self.hover_preview_enabled = !self.hover_preview_enabled;
                }
                if key_l {
                    self.magnifier_scope_active = !self.magnifier_scope_active;
                    if self.magnifier_scope_active {
                        self.denoiser_panel_active = false;
                    }
                }
                if ctx.input(|i| !i.modifiers.command && i.key_pressed(egui::Key::F)) {
                    self.denoiser_panel_active = !self.denoiser_panel_active;
                    if self.denoiser_panel_active {
                        self.magnifier_scope_active = false;
                        self.refresh_noise_presets();
                    }
                }
                if let Some(m_idx) = self.selected_marker_idx {
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
                if key_v || key_esc {
                    self.active_edit_tool = EditTool::Pointer;
                }
                if key_del {
                    if let Some(index) = self.selected_marker_idx {
                        self.request_marker_delete(index);
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        let needs_menu = self.mac_menu.is_none();
        #[cfg(not(target_os = "macos"))]
        let needs_menu = true;
        if needs_menu {
        egui::TopBottomPanel::top("app_menu_bar")
            .frame(egui::Frame::none().fill(Color32::from_rgb(20, 22, 26)).inner_margin(egui::Margin { left: 12.0, right: 12.0, top: 4.0, bottom: 4.0 }))
            .show(ctx, |ui| {
                if self.show_export { ui.disable(); }
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(self.language.menu_file(), |ui| {
                        if ui.button(format!("{} (⌘N)", self.language.menu_new_project())).clicked() {
                            self.new_project();
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (⌘O)", self.language.menu_open_project())).clicked() {
                            self.open_project_dialog(ctx);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(format!("{} (⌘S)", self.language.menu_save_project())).clicked() {
                            self.save_project_quick(ctx);
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (⇧⌘S)", self.language.menu_save_project_as())).clicked() {
                            self.save_project_dialog(ctx);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(self.language.menu_reveal_in_finder()).clicked() {
                            self.reveal_session_in_finder();
                            ui.close_menu();
                        }
                        ui.separator();
                        let export_lbl = self.text("Tracks exportieren … (⇧⌘E)", "Export tracks … (⇧⌘E)");
                        if ui.add_enabled(self.player.total_samples() > 0 && !self.recorder.is_busy(), egui::Button::new(export_lbl)).clicked() {
                            self.active_tab = ActiveTab::SplitAndExport;
                            self.export_page = 0;
                            self.show_export = true;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(self.language.menu_edit(), |ui| {
                        if ui.button(format!("{} (⌘Z)", self.language.menu_undo())).clicked() {
                            self.undo_marker_edit(false);
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (⇧⌘Z)", self.language.menu_redo())).clicked() {
                            self.undo_marker_edit(true);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(self.language.menu_view(), |ui| {
                        if ui.button(format!("{} (⌘1)", self.text("Aufnahme", "Record"))).clicked() {
                            self.active_tab = ActiveTab::Recording;
                            ui.close_menu();
                        }
                        if ui.add_enabled(!self.recorder.is_busy(), egui::Button::new(format!("{} (⌘2)", self.text("Pegel", "Levels")))).clicked() {
                            self.active_tab = ActiveTab::Diagnosis;
                            ui.close_menu();
                        }
                        if ui.add_enabled(!self.recorder.is_busy(), egui::Button::new(format!("{} (⌘3)", self.text("Schnitt", "Edit")))).clicked() {
                            self.active_tab = ActiveTab::SplitAndExport;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(format!("{} (F)", self.language.menu_toggle_filter_studio())).clicked() {
                            self.denoiser_panel_active = !self.denoiser_panel_active;
                            if self.denoiser_panel_active {
                                self.magnifier_scope_active = false;
                                self.refresh_noise_presets();
                                self.active_tab = ActiveTab::SplitAndExport;
                            }
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (L)", self.language.menu_toggle_scope())).clicked() {
                            self.magnifier_scope_active = !self.magnifier_scope_active;
                            if self.magnifier_scope_active {
                                self.denoiser_panel_active = false;
                                self.active_tab = ActiveTab::SplitAndExport;
                            }
                            ui.close_menu();
                        }
                        if ui.button(self.language.menu_toggle_tracks()).clicked() {
                            self.show_tracks = !self.show_tracks;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(self.language.menu_filter(), |ui| {
                        if ui.button(format!("{} (F)", self.language.menu_toggle_filter_studio())).clicked() {
                            self.denoiser_panel_active = !self.denoiser_panel_active;
                            if self.denoiser_panel_active {
                                self.magnifier_scope_active = false;
                                self.refresh_noise_presets();
                                self.active_tab = ActiveTab::SplitAndExport;
                            }
                            ui.close_menu();
                        }
                        let live_lbl = if self.denoiser_params.enabled {
                            self.text("Echtzeit-Filter: AUS schalten", "Turn Real-Time Filter OFF")
                        } else {
                            self.text("Echtzeit-Filter: AN schalten", "Turn Real-Time Filter ON")
                        };
                        if ui.button(live_lbl).clicked() {
                            self.denoiser_params.enabled = !self.denoiser_params.enabled;
                            self.player.set_denoiser_params(self.denoiser_params.clone());
                            ui.close_menu();
                        }
                        if ui.add_enabled(self.player.total_samples() > 0, egui::Button::new(self.language.menu_learn_noise())).clicked() {
                            let total_sec = self.player.total_seconds();
                            let cur_sec = self.player.current_seconds();
                            let (s0, s1) = self.active_selection_or_window(total_sec, cur_sec);
                            self.learn_noise_from_region(s0, s1);
                            ui.close_menu();
                        }
                        ui.separator();
                        let has_audio = self.player.total_samples() > 0;
                        let can_bake = has_audio && (self.active_noise_profile.is_some() || self.denoiser_params.dc_filter_enabled);
                        let total_sec = self.player.total_seconds();
                        let cur_sec = self.player.current_seconds();
                        let tracks = self.splitter.get_tracks(total_sec);
                        let cur_track = self.selected_segment_idx.or_else(|| {
                            tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds).map(|t| t.segment_index)
                        }).and_then(|idx| tracks.iter().find(|t| t.segment_index == idx && !t.is_pause));

                        let bake_track_lbl = if let Some(ref t) = cur_track {
                            format!("{} (Track {:02})", self.language.menu_bake_selected_track(), t.index)
                        } else {
                            self.language.menu_bake_selected_track().to_string()
                        };
                        if ui.add_enabled(can_bake && cur_track.is_some(), egui::Button::new(bake_track_lbl)).clicked() {
                            if let Some(t) = cur_track {
                                self.bake_denoiser_into_track(ctx, t.segment_index);
                            }
                            ui.close_menu();
                        }
                        if ui.add_enabled(can_bake, egui::Button::new(self.language.menu_bake_all())).clicked() {
                            self.bake_denoiser_into_recording(ctx);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.add_enabled(!tracks.is_empty(), egui::Button::new(self.language.menu_filter_all_tracks())).clicked() {
                            self.splitter.set_all_tracks_filtered(tracks.len(), true);
                            ui.close_menu();
                        }
                        if ui.add_enabled(!tracks.is_empty(), egui::Button::new(self.language.menu_filter_no_tracks())).clicked() {
                            self.splitter.set_all_tracks_filtered(tracks.len(), false);
                            ui.close_menu();
                        }
                        if ui.add_enabled(!tracks.is_empty(), egui::Button::new(self.language.menu_toggle_track_filter())).clicked() {
                            let seg_idx = self.selected_segment_idx.unwrap_or(0);
                            self.splitter.toggle_track_filter(seg_idx);
                            ui.close_menu();
                        }
                        ui.separator();
                        let hp_lbl = if self.export_highpass_20hz {
                            self.text("20 Hz Subsonic-Filter: AUS schalten", "Turn 20 Hz Subsonic Filter OFF")
                        } else {
                            self.text("20 Hz Subsonic-Filter: AN schalten", "Turn 20 Hz Subsonic Filter ON")
                        };
                        if ui.button(hp_lbl).clicked() {
                            self.export_highpass_20hz = !self.export_highpass_20hz;
                            self.save_config();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(self.language.menu_help(), |ui| {
                        if ui.button(self.language.menu_about()).clicked() {
                            rfd::MessageDialog::new().set_title("TapeDigizer")
                                .set_description(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .show();
                            ui.close_menu();
                        }
                    });
                });
            });

        }

        egui::TopBottomPanel::top("workspace_toolbar")
            .frame(egui::Frame::none().fill(BG_PANEL)
                .stroke(Stroke::new(1.0, BORDER_MID)).inner_margin(10.0))
            .show(ctx, |ui| {
                if self.show_export { ui.disable(); }
                ui.horizontal_wrapped(|ui| {
                    studio_wordmark(ui);
                    ui.separator();
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (tab, label) in [
                            (ActiveTab::Recording, self.text("Aufnahme · ⌘1", "Record · ⌘1")),
                            (ActiveTab::Diagnosis, self.text("Pegel · ⌘2", "Levels · ⌘2")),
                            (ActiveTab::SplitAndExport, self.text("Schnitt · ⌘3", "Edit · ⌘3")),
                        ] {
                            if ui.add_enabled(!self.recorder.is_busy() || tab == ActiveTab::Recording, mode_button(label, self.active_tab == tab)).clicked() {
                                self.active_tab = tab;
                            }
                        }
                    });
                    ui.separator();
                    if waiting_for_signal {
                        ui.spinner();
                        ui.label(self.text("Warte auf Eingangssignal …", "Waiting for input signal …"));
                    } else if run_on {
                        let seconds = self.recorder.remaining_duration()
                            .map_or(0, |remaining| remaining.as_secs_f64().ceil() as u64);
                        let label = self.text("Nachlauf", "Recording tail");
                        ui.colored_label(LED_AMBER, format!("● {label} {seconds} s"));
                    } else if is_recording {
                        let time = self.recorder.elapsed_duration().as_secs_f64();
                        ui.colored_label(LED_RED,
                            format!("● REC {}", timecode(time)));
                    } else if finalizing {
                        ui.spinner();
                        ui.label(self.text("WAV wird abgeschlossen …", "Finalizing WAV …"));
                    } else if is_monitoring {
                        ui.colored_label(LED_GREEN, self.text("● Pegelcheck aktiv", "● Monitoring"));
                    } else if self.current_audio_path.is_some() {
                        ui.label(format!("{} · {}", self.tape_name, self.tape_side.label(self.language)));
                        if self.saved_edits.as_ref() != Some(&self.edits()) {
                            ui.label("•").on_hover_text(self.text("Ungespeicherte Änderungen", "Unsaved changes"));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.language.code()).on_hover_text(self.language.display_name()).clicked() {
                            self.language = if self.language == Language::De { Language::En } else { Language::De };
                            self.save_config();
                        }
                        if self.current_audio_path.is_none() && !self.recorder.is_busy() && self.job.is_none() {
                            if ui.button(self.text("WAV / Projekt öffnen · ⌘O", "Open WAV / project · ⌘O")).clicked() {
                                self.open_project_dialog(ctx);
                            }
                        }
                    });
                });
                if self.active_tab == ActiveTab::Recording {
                    ui.add_space(5.0);
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let stop_label = if self.run_on_seconds > 0 {
                            if self.language == Language::De {
                                format!("Stopp ({}s Nachlauf)", self.run_on_seconds)
                            } else {
                                format!("Stop ({}s tail)", self.run_on_seconds)
                            }
                        } else {
                            self.text("Aufnahme stoppen", "Stop recording").to_string()
                        };
                        let label = match recorder_state {
                            RecorderState::WaitingForSignal => self.text("Warten abbrechen", "Cancel waiting").to_string(),
                            RecorderState::Recording => stop_label,
                            RecorderState::RunOn => self.text("Nachlauf läuft …", "Recording tail …").to_string(),
                            _ => self.text("Aufnahme starten", "Start recording").to_string(),
                        };
                        let record = record_button(&label);
                        let ready = waiting_for_signal || recorder_state == RecorderState::Recording
                            || (!self.recorder.is_busy() && self.job.is_none() && self.current_channel_pair().is_some());
                        if ui.add_enabled(ready, record).clicked() {
                            self.record_or_stop();
                        }
                        if matches!(recorder_state, RecorderState::Recording | RecorderState::RunOn)
                            && (self.run_on_seconds > 0 || recorder_state == RecorderState::RunOn)
                        {
                            if ui.button(self.text("⏹ Sofort beenden", "⏹ Stop immediately")).clicked() {
                                self.recorder.stop_immediately();
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new(self.media_source.label(self.language)).color(ACCENT));
                        if let Some(device) = self.current_device() {
                            ui.label(&device.name);
                        }
                        if let Some(pair) = self.current_channel_pair() {
                            ui.label(RichText::new(pair.label).color(TEXT_SECONDARY));
                        }
                    });
                }
                if self.active_tab == ActiveTab::SplitAndExport {
                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_enabled_ui(self.job.is_none() && !self.recorder.is_busy(), |ui| self.render_transport(ui));
                }
            });

        // Main Studio Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG_WINDOW).inner_margin(16.0))
            .show(ctx, |ui| {
                if let Some(ref err) = self.ui_error_message {
                    let err_frame = egui::Frame::none()
                        .fill(Color32::from_rgb(50, 24, 26))
                        .stroke(Stroke::new(1.0, LED_RED))
                        .rounding(6.0)
                        .inner_margin(10.0);
                    err_frame.show(ui, |ui| {
                        ui.colored_label(LED_RED, format!("⚠️ {}", err));
                    });
                    ui.add_space(8.0);
                }

                if let Some(job) = self.job.as_ref().filter(|job| job.export_progress.is_none()) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(job.label);
                    });
                }
                let enabled = !self.show_export && self.job.is_none() && !finalizing && (!self.recorder.is_busy() || self.active_tab == ActiveTab::Recording);
                ui.add_enabled_ui(enabled, |ui| {
                    match self.active_tab {
                        ActiveTab::Recording => self.render_tab_recording(ui),
                        ActiveTab::Diagnosis => self.render_tab_diagnosis(ui),
                        ActiveTab::SplitAndExport => self.render_tab_split_export(ui),
                    }
                });
            });
        if let Some((index, seconds, title)) = self.pending_delete_marker.take() {
            self.clear_hover_audition();
            let confirmed = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title(self.text("Schnittmarke entfernen?", "Remove cut marker?"))
                .set_description(self.language.confirm_delete_question(
                    (seconds / 60.0) as u64, (seconds % 60.0) as u64, &title,
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show() == rfd::MessageDialogResult::Yes;
            ctx.request_repaint();
            if confirmed && self.splitter.markers.get(index) == Some(&seconds) {
                self.splitter.remove_marker_at_index(index);
                self.selected_marker_idx = None;
                self.selected_segment_idx = None;
            }
        }
        if self.show_export {
            self.clear_hover_audition();
            self.render_export_dialog(ctx);
        }
        if self.active_tab != ActiveTab::SplitAndExport {
            self.clear_hover_audition();
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
            let play = if self.player.is_playing() { self.language.btn_pause() } else { self.language.btn_play() };
            if ui.add_enabled(total > 0.0, egui::Button::new(play)).clicked() {
                self.player.toggle_play();
            }
            if ui.add_enabled(total > 0.0, egui::Button::new(self.language.btn_stop())).clicked() {
                self.player.stop();
            }
            time_display(ui, format!("{}  /  {}", timecode(position), timecode(total)));
            ui.separator();
            if ui.add_enabled(total > 0.0, egui::Button::new(self.text("│ Marker setzen · M", "│ Add marker · M"))).clicked()
            {
                self.add_marker(position);
            }
            let lupe = self.text("🔍 Schnitt-Lupe · L", "🔍 Cut Magnifier · L");
            if ui.toggle_value(&mut self.magnifier_scope_active, lupe).clicked() && self.magnifier_scope_active {
                self.denoiser_panel_active = false;
            }
            let denoise_lbl = self.text("🔇 Rausch- & DC-Filter · F", "🔇 Noise & DC Filter · F");
            if ui.toggle_value(&mut self.denoiser_panel_active, denoise_lbl).clicked() && self.denoiser_panel_active {
                self.magnifier_scope_active = false;
                self.refresh_noise_presets();
            }
            let tracks = self.text("Trackliste", "Track list");
            ui.toggle_value(&mut self.show_tracks, tracks);
            if ui.add_enabled(total > 0.0, primary_button(self.text("Exportieren … · ⇧⌘E", "Export … · ⇧⌘E"))).clicked() {
                self.export_page = 0;
                self.show_export = true;
            }
        });
    }

    fn add_marker(&mut self, position: f64) {
        let snapped = self.snap_time_if_enabled(position);
        if let Some(index) = self.splitter.add_marker(snapped, self.player.total_seconds(), self.player.sample_rate()) {
            self.select_marker(index);
            self.magnifier_scope_active = true;
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
        if let Some(&time) = self.splitter.markers.get(index) {
            let title = self.splitter.custom_titles.get(index + 1)
                .filter(|title| !title.is_empty()).cloned()
                .unwrap_or_else(|| format!("Track {:02}", index + 2));
            self.pending_delete_marker = Some((index, time, title));
        }
    }

    // -------------------------------------------------------------
    // TAB 1: Aufnahme & Einpegeln
    // -------------------------------------------------------------
    fn render_tab_recording(&mut self, ui: &mut Ui) {
        ui.columns(2, |columns| {
            let setup = &mut columns[0];
            let editable = !self.recorder.is_busy();
            setup.add_enabled_ui(editable, |ui| {
                studio_panel(ui, self.text("Quelle & Projekt", "Source & project"), |ui| {
                    // Media Source Presets (Kassette, Tonband, Vinyl LP, Single 45)
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(self.text("Quelle:", "Source:")).color(TEXT_SECONDARY));
                        for src in [
                            MediaSourceType::Cassette,
                            MediaSourceType::ReelToReel,
                            MediaSourceType::VinylLp,
                            MediaSourceType::VinylSingle,
                        ] {
                            if ui.add(mode_button(src.label(self.language), self.media_source == src)).clicked() {
                                self.apply_media_source_preset(src);
                            }
                        }
                    });
                    ui.add_space(4.0);

                    // Album/Bandname + Interpret/Artist side-by-side
                    ui.columns(2, |cols| {
                        cols[0].label(RichText::new(self.text("Medium / Album", "Tape / album")).color(TEXT_SECONDARY).small());
                        let w0 = cols[0].available_width();
                        let name_resp = cols[0].add(
                            egui::TextEdit::singleline(&mut self.tape_name)
                                .desired_width(w0)
                                .hint_text(self.language.hint_tape_name()),
                        );
                        if name_resp.lost_focus() {
                            self.save_config();
                        }

                        cols[1].label(RichText::new(self.text("Interpret (optional)", "Artist (optional)")).color(TEXT_SECONDARY).small());
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
                        for side in [TapeSide::SideA, TapeSide::SideB] {
                            if ui.add(mode_button(side.label(self.language), self.tape_side == side)).clicked() {
                                self.tape_side = side;
                            }
                        }
                        if ui.button(self.language.btn_change_folder()).clicked() {
                            if let Some(folder) = rfd::FileDialog::new().set_directory(&self.export_directory).pick_folder() {
                                self.export_directory = folder;
                                self.save_config();
                            }
                        }
                        if ui.button(self.text("Im Finder", "Reveal")).clicked() {
                            self.reveal_session_in_finder();
                        }
                    });
                    let folder = self.session_project_dir().display().to_string();
                    ui.add(egui::Label::new(RichText::new(&folder).color(TEXT_SECONDARY).small()).truncate())
                        .on_hover_text(folder);
                });

                ui.add_space(7.0);
                studio_panel(ui, self.text("Audioeingang", "Audio input"), |ui| {
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
                            if let Some(device) = self.current_device() {
                                self.channel_pairs = get_channel_pairs_for_device(device);
                                self.selected_pair_idx = 0;
                                self.save_config();
                            }
                        }
                        if ui.button(format!("↻ {}", self.language.btn_rescan())).clicked() {
                            self.refresh_device_list();
                        }
                    });

                    let channels = self.current_channel_pair().map(|pair| pair.label)
                        .unwrap_or_else(|| self.language.label_select_channel().into());
                    let selected = self.selected_pair_idx;
                    egui::ComboBox::from_id_salt("channel_combo")
                        .width(ui.available_width()).truncate().selected_text(channels)
                        .show_ui(ui, |ui| {
                            for (index, pair) in self.channel_pairs.iter().enumerate() {
                                ui.selectable_value(&mut self.selected_pair_idx, index, &pair.label);
                            }
                        });
                    if self.selected_pair_idx != selected {
                        self.save_config();
                    }
                });

                ui.add_space(7.0);
                studio_panel(ui, self.text("Aufnahmedauer & Auto-Stopp", "Duration & auto-stop"), |ui| {
                    let before_timer = self.selected_timer_minutes;
                    let before_headroom = self.time_headroom_minutes;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(self.text("Dauer:", "Length:")).color(TEXT_SECONDARY).small());
                        for (minutes, label) in [
                            (10, "10m"), (25, "25m"), (30, "30m"), (45, "45m"), (60, "60m"), (90, "90m"),
                            (0, self.language.preset_unlimited()),
                        ] {
                            if ui.add(mode_button(label, self.selected_timer_minutes == minutes)).clicked() {
                                self.selected_timer_minutes = minutes;
                            }
                        }
                    });

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(self.text("Zeit-Headroom:", "Time Headroom:")).color(TEXT_SECONDARY).small());
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

                    let summary_text = if self.selected_timer_minutes > 0 {
                        let total = self.selected_timer_minutes + self.time_headroom_minutes;
                        format!(
                            "⏱ {} min + {} min {} = {}:00 min max.",
                            self.selected_timer_minutes,
                            self.time_headroom_minutes,
                            self.text("Headroom", "Headroom"),
                            total
                        )
                    } else {
                        self.text("⏱ Unbegrenzt (kein Zeitlimit)", "⏱ Unlimited (no time limit)").to_string()
                    };
                    ui.label(RichText::new(summary_text).color(LED_AMBER).small());

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
                        ui.label(RichText::new(self.text("Nachlauf:", "Tail:")).color(TEXT_SECONDARY).small());
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
                            ui.label(RichText::new(self.text("Schwelle:", "Threshold:")).color(TEXT_SECONDARY).small());
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

                        ui.label(RichText::new(self.text(
                            "💡 Tipp: -100 dBFS sucht nach fehlendem Signal und verhindert vorzeitiges Abbrechen bei Bandrauschen oder Pausen.",
                            "💡 Tip: -100 dBFS searches for lost signal and prevents early cutoff during tape hiss or gaps.",
                        )).color(TEXT_SECONDARY).small());
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

            let signal = &mut columns[1];
            studio_panel(signal, self.text("Eingangspegel", "Input levels"), |ui| {
                let monitoring = self.recorder.state() == RecorderState::Monitoring;
                let meter_snap = self.recorder.get_meter();
                ui.horizontal_wrapped(|ui| {
                    let label = if monitoring {
                        self.text("⏹ Pegelcheck beenden", "⏹ End level check")
                    } else {
                        self.text("▶ Pegel prüfen (Monitoring)", "▶ Check levels")
                    };
                    if ui.add_enabled(editable && (monitoring || self.current_channel_pair().is_some()),
                        mode_button(label, monitoring)).clicked()
                    {
                        if monitoring {
                            self.recorder.stop_monitoring();
                        } else if let (Some(device), Some(pair)) = (self.current_device(), self.current_channel_pair()) {
                            let name = device.name.clone();
                            self.ui_error_message = self.recorder.start_monitoring(&name, pair).err();
                        }
                    }
                    if ui.small_button(self.text("Peak/Clip Reset", "Reset peaks"))
                        .on_hover_text(self.text("Gehaltene Spitzenwerte & Clip-Anzeige zurücksetzen", "Reset peak hold & clip indicator"))
                        .clicked()
                    {
                        self.recorder.reset_meter_peaks();
                    }
                    if meter_snap.peak_hold_l_db > -60.0 || meter_snap.peak_hold_r_db > -60.0 {
                        let diff = meter_snap.peak_hold_l_db - meter_snap.peak_hold_r_db;
                        let bal_col = if diff.abs() > 3.0 { LED_AMBER } else { TEXT_SECONDARY };
                        ui.label(RichText::new(format!("Δ L/R: {:+.1} dB", diff)).monospace().small().color(bal_col));
                    }
                });
                if render_stereo_meter(ui, &meter_snap) {
                    self.recorder.reset_meter_peaks();
                }
                ui.label(RichText::new(self.language.meter_instructions()).color(TEXT_SECONDARY).small());
            });
            signal.add_space(7.0);
            studio_panel(signal, self.text("Live-Wellenform", "Live waveform"), |ui| {
                let state = self.recorder.state();
                let recording = matches!(state, RecorderState::Recording | RecorderState::RunOn);
                let elapsed = self.recorder.elapsed_duration().as_secs_f64();
                let limit = (self.selected_timer_minutes > 0)
                    .then(|| f64::from(self.selected_timer_minutes + self.time_headroom_minutes) * 60.0 + self.run_on_seconds as f64);
                let idle = match state {
                    RecorderState::WaitingForSignal => self.text("Warte auf Eingangssignal …", "Waiting for input signal …"),
                    RecorderState::Monitoring => self.language.live_waveform_monitoring(),
                    _ => self.language.live_waveform_idle(),
                };
                self.recorder.with_waveform_data(|points| {
                    crate::ui::waveform_widget::render_live_recording_waveform(ui, points, elapsed, limit, recording, idle);
                });
                ui.horizontal_wrapped(|ui| {
                    time_display(ui, timecode(elapsed));
                    if let Some(remaining) = self.recorder.remaining_duration() {
                        let label = self.text("Verbleibend", "Remaining");
                        ui.label(format!("{label}  {}", timecode(remaining.as_secs_f64())));
                    }
                    if recording && self.enable_silence_autostop {
                        let sil = self.recorder.silence_seconds_detected();
                        if sil > 0.5 {
                            let sil_msg = format!("⏳ Stille: {:.1}s / {:.0}s ({:.0} dB)", sil, self.autostop_silence_secs, self.autostop_threshold_db);
                            ui.label(RichText::new(sil_msg).color(LED_AMBER).small());
                        }
                    }
                    ui.label(RichText::new(format!("PCM 24-bit · Stereo · {}", self.media_source.label(self.language))).color(TEXT_SECONDARY));
                });
            });
        });
    }

    // -------------------------------------------------------------
    // TAB 2: Pegel-Diagnose & Regler-Empfehlung
    // -------------------------------------------------------------
    fn render_tab_diagnosis(&mut self, ui: &mut Ui) {
        ui.add_space(2.0);

        let Some(report) = self.analysis_report.clone() else {
            ui.heading(if self.language == Language::De { "Noch keine Aufnahme analysiert." } else { "No recording analyzed yet." });
            if let Some(ref err) = self.ui_error_message {
                ui.add_space(8.0);
                ui.colored_label(LED_RED, format!("⚠️ {}", err));
            } else {
                ui.label(if self.language == Language::De { "Starte zuerst eine Aufnahme in Tab 1 oder öffne eine WAV-Datei (⌘O)." } else { "Start a recording in Tab 1 first or open a WAV file (⌘O)." });
            }
            ui.add_space(12.0);
            if ui.button(if self.language == Language::De { "Zurück zu Tab 1 (Aufnahme)" } else { "Back to Tab 1 (Recording)" }).clicked() {
                self.active_tab = ActiveTab::Recording;
            }
            return;
        };

        // Top 2-column layout: Left = Analog Gain Knob Card, Right = Stereo Balance & Signal Health
        let (adjustment_db, is_clip) = match report.diagnosis {
            LevelDiagnosis::Clipped { recommended_cut_db, .. } => (-recommended_cut_db, true),
            LevelDiagnosis::TooQuiet { recommended_boost_db, .. } => (recommended_boost_db, false),
            LevelDiagnosis::Optimal { .. } => (0.0, false),
        };

        ui.columns(2, |cols| {
            render_gain_knob_card(
                &mut cols[0],
                self.language.card_knob_title(),
                report.max_peak_db,
                adjustment_db,
                is_clip,
            );

            studio_panel(&mut cols[1], self.text("Stereo-Balance & Analog-Diagnose", "Stereo balance & analog diagnostics"), |ui| {
                let bal = report.stereo_balance_db;
                let abs_bal = bal.abs();
                let (bal_text, bal_col) = if abs_bal < 0.8 {
                    (self.text("Ausgewogen (L ≈ R)", "Balanced (L ≈ R)").to_string(), LED_GREEN)
                } else if bal > 0.0 {
                    (format!("Links +{:.1} dB lauter", bal), if abs_bal > 2.5 { LED_AMBER } else { TEXT_PRIMARY })
                } else {
                    (format!("Rechts +{:.1} dB lauter", -bal), if abs_bal > 2.5 { LED_AMBER } else { TEXT_PRIMARY })
                };

                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(self.text("L/R Balance:", "L/R Balance:")).color(TEXT_SECONDARY));
                    ui.label(RichText::new(bal_text).strong().monospace().color(bal_col));
                    if abs_bal >= 0.5 {
                        let active = (self.export_balance_db + bal).abs() < 0.2 && self.export_balance_db.abs() >= 0.2;
                        let btn_lbl = if active {
                            self.text("✓ L/R-Ausgleich aktiv", "✓ L/R trim active")
                        } else {
                            self.text("Beim Export ausgleichen", "Compensate on export")
                        };
                        if ui.add(mode_button(btn_lbl, active)).clicked() {
                            self.export_balance_db = if active { 0.0 } else { (-bal).clamp(-12.0, 12.0) };
                        }
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!(
                        "RMS L: {:.1} dB  ·  RMS R: {:.1} dB  ·  Dynamik (Crest): {:.1} dB  ·  DC-Offset: {:.2}%",
                        report.rms_left_db, report.rms_right_db, report.crest_factor_db, report.dc_offset_percent
                    )).monospace().small().color(TEXT_SECONDARY));
                });

                ui.horizontal_wrapped(|ui| {
                    let hp_lbl = self.text("20 Hz Subsonic-/Rumpelfilter (ideal für Schallplatte & Tonband)", "20 Hz subsonic rumble & DC filter");
                    if ui.checkbox(&mut self.export_highpass_20hz, hp_lbl).changed() {
                        self.save_config();
                    }
                });
            });
        });

        ui.add_space(12.0);

        // 4 Hardware Stat Cards
        ui.horizontal(|ui| {
            let card_w = (ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0;

            let l_label = if self.language == Language::De { "PEAK KANAL 1 (L)" } else { "PEAK CHANNEL 1 (L)" };
            render_hardware_metric(ui, card_w, l_label, &format!("{:.1} dBFS", report.peak_left_db), Color32::from_rgb(56, 189, 248));

            let r_label = if self.language == Language::De { "PEAK KANAL 2 (R)" } else { "PEAK CHANNEL 2 (R)" };
            render_hardware_metric(ui, card_w, r_label, &format!("{:.1} dBFS", report.peak_right_db), Color32::from_rgb(147, 130, 248));

            let headroom = (-1.5 - report.max_peak_db).max(0.0);
            let head_label = if self.language == Language::De { "HEADROOM (-1.5 dB Ziel)" } else { "HEADROOM (-1.5 dB Target)" };
            render_hardware_metric(ui, card_w, head_label, &format!("{:.1} dB", headroom), LED_AMBER);

            let (clip_str, clip_col) = if report.clipped_samples > 0 {
                (format!("{} Samples", report.clipped_samples), LED_RED)
            } else {
                ((if self.language == Language::De { "0 (Sauber)" } else { "0 (Clean)" }).into(), LED_GREEN)
            };
            let clip_label = if self.language == Language::De { "CLIPPING (VERZERRUNG)" } else { "CLIPPING (DISTORTION)" };
            render_hardware_metric(ui, card_w, clip_label, &clip_str, clip_col);
        });

        ui.add_space(14.0);

        ui.horizontal_wrapped(|ui| {
            if ui.button(self.language.btn_retry()).clicked() {
                self.active_tab = ActiveTab::Recording;
            }
            if ui.add(primary_button(self.language.btn_proceed_to_split())).clicked() {
                self.active_tab = ActiveTab::SplitAndExport;
            }
        });
    }

    // -------------------------------------------------------------
    // TAB 3: Schnitt & Export
    // -------------------------------------------------------------
    fn render_tab_split_export(&mut self, ui: &mut Ui) {
        let total_sec = self.player.total_seconds();
        let cur_sec = self.player.current_seconds();
        let has_audio = total_sec > 0.1 && !self.waveform_cache.levels.is_empty();

        if !has_audio {
            egui::Frame::none()
                .fill(Color32::from_rgb(34, 36, 44))
                .stroke(Stroke::new(1.0, BORDER_MID))
                .rounding(5.0)
                .inner_margin(Vec2::new(12.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ℹ").monospace().strong().color(LED_AMBER));
                        let note = if self.language == Language::De {
                            "Noch keine Audiodatei geladen. Starte eine Aufnahme in Tab 1 oder öffne ein gespeichertes Projekt / WAV (⌘O)."
                        } else {
                            "No audio recorded or loaded yet. Start recording in Tab 1 or open an existing project / WAV (⌘O)."
                        };
                        ui.label(RichText::new(note).color(TEXT_SECONDARY).small());
                    });
                });
            ui.add_space(4.0);
        }

        let show_scope = self.magnifier_scope_active && has_audio;
        let show_denoiser = self.denoiser_panel_active && has_audio;
        let wf_height = (ui.available_height() * 0.34).clamp(124.0, 240.0);

        // Full-width main Stereo Waveform always at the top
        self.render_timeline(ui, total_sec, cur_sec, has_audio, wf_height);
        ui.add_space(4.0);

        let tracks = self.splitter.get_tracks(total_sec);
        if show_denoiser && self.show_tracks {
            ui.columns(2, |columns| {
                self.render_track_list(&mut columns[0], &tracks);
                self.render_denoiser_and_cut_panel(&mut columns[1], total_sec, cur_sec);
            });
        } else if show_denoiser {
            self.render_denoiser_and_cut_panel(ui, total_sec, cur_sec);
        } else if show_scope && self.show_tracks {
            ui.columns(2, |columns| {
                self.render_track_list(&mut columns[0], &tracks);
                self.render_detail_panel(&mut columns[1], total_sec, cur_sec);
            });
        } else if show_scope {
            self.render_detail_panel(ui, total_sec, cur_sec);
        } else {
            self.render_track_list(ui, &tracks);
        }
    }

    fn render_timeline(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64, has_audio: bool, waveform_height: f32) {
        studio_panel(ui, self.text("Stereo-Wellenform (Gesamtübersicht)", "Stereo waveform (Overview)"), |ui| {
            // Top compact bar: Edit tools + Hover Audition + Schnitt-Lupe + Zero-Crossing + Undo/Redo + Zoom
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tool, name, key) in [
                    (EditTool::Pointer, self.text("↖ Zeiger", "↖ Pointer"), "V"),
                    (EditTool::Scissor, self.text("│ Schnitt", "│ Split"), "S"),
                    (EditTool::Select, self.text("Bereich", "Range"), "R"),
                ] {
                    if ui.add(mode_button(format!("{name}·{key}"), self.active_edit_tool == tool)).clicked() {
                        self.active_edit_tool = tool;
                    }
                }
                ui.separator();
                let preview = self.text("Vorhören", "Audition");
                let detail = self.text("🔍 Lupe", "🔍 Scope");
                let flt = self.text("🔇 Filter", "🔇 Filter");
                let zc_lbl = self.text("0-Snap", "0-Snap");
                let zc_tip = self.text("Schnittmarken automatisch auf Nulldurchgang einrasten (verhindert Knackser)", "Snap markers to zero crossings to prevent clicks");
                ui.toggle_value(&mut self.hover_preview_enabled, format!("{preview}·H"));
                if ui.toggle_value(&mut self.magnifier_scope_active, format!("{detail}·L")).clicked() && self.magnifier_scope_active {
                    self.denoiser_panel_active = false;
                }
                if ui.toggle_value(&mut self.denoiser_panel_active, format!("{flt}·F")).clicked() && self.denoiser_panel_active {
                    self.magnifier_scope_active = false;
                    self.refresh_noise_presets();
                }
                if ui.checkbox(&mut self.zero_crossing_snap, zc_lbl)
                    .on_hover_text(zc_tip)
                    .changed()
                {
                    self.save_config();
                }
                ui.separator();
                if ui.add_enabled(self.splitter.can_undo(), egui::Button::new("↶").small())
                    .on_hover_text(self.language.btn_undo()).clicked()
                {
                    self.undo_marker_edit(false);
                }
                if ui.add_enabled(self.splitter.can_redo(), egui::Button::new("↷").small())
                    .on_hover_text(self.language.btn_redo()).clicked()
                {
                    self.undo_marker_edit(true);
                }
                ui.separator();
                ui.spacing_mut().slider_width = 48.0;
                ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0)
                    .text("Zoom").suffix("×"));
                let gain = self.text("Pegel", "Gain");
                ui.add(egui::Slider::new(&mut self.waveform_vertical_zoom, 1.0..=16.0)
                    .text(gain).suffix("×"));
                if ui.small_button("1:1").on_hover_text(self.text("Ganzes Band anzeigen", "Fit entire tape")).clicked() {
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
            );

            if let Some(index) = selected_marker.filter(|&i| self.selected_marker_idx != Some(i)) {
                self.select_marker(index);
            }
            if let Some((r_start, r_end)) = interaction.new_selected_region {
                self.set_scope_region((r_start, r_end));
                if !self.denoiser_panel_active {
                    self.magnifier_scope_active = true;
                }
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
                ui.label(RichText::new(self.text("Pausen:", "Pauses:")).color(TEXT_SECONDARY).small());
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

                if ui.add_enabled(has_audio, egui::Button::new(self.text("🔍 Suchen", "🔍 Detect")).small()).clicked() {
                    self.preview_pauses(ui.ctx(), self.pause_as_gaps);
                }

                if let Some((cuts, gaps)) = self.pause_preview.as_ref().map(|p| (p.markers.len(), p.pause_indices.len())) {
                    ui.separator();
                    ui.colored_label(
                        LED_AMBER,
                        RichText::new(format!("{cuts} Schnitte ({gaps}⏸)")).small(),
                    );
                    if ui.add(egui::Button::new(RichText::new(self.text("✓ Übernehmen", "✓ Apply")).color(Color32::BLACK).strong()).fill(ACCENT).small())
                        .on_hover_text(self.text("Ersetzt die Trackliste. Mit ⌘Z rückgängig machen.", "Replaces the track list. Undo with ⌘Z."))
                        .clicked()
                    {
                        let before = self.splitter.snapshot();
                        self.splitter.restore(self.pause_preview.take().unwrap());
                        self.splitter.commit_edit(before);
                        self.reset_editor_selection();
                    }
                    if ui.small_button("×").on_hover_text(self.text("Vorschlag verwerfen", "Discard proposal")).clicked() {
                        self.pause_preview = None;
                    }
                }

                if ui.add_enabled(has_audio && !self.splitter.markers.is_empty(),
                    egui::Button::new(self.text("Alle löschen", "Clear all")).small()).clicked()
                {
                    self.splitter.clear_markers();
                    self.selected_marker_idx = None;
                    self.selected_segment_idx = None;
                }
            });
        });
    }

    fn render_detail_panel(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64) {
        // Auto-select closest marker if none is selected and markers exist
        if self.selected_marker_idx.is_none() && !self.splitter.markers.is_empty() {
            if let Some((closest_idx, _)) = self.splitter.markers.iter().enumerate().min_by(|(_, a), (_, b)| {
                (*a - cur_sec).abs().total_cmp(&(*b - cur_sec).abs())
            }) {
                self.selected_marker_idx = Some(closest_idx);
                self.selected_segment_idx = Some(closest_idx + 1);
            }
        }

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

        studio_panel(ui, self.text("Präzisions-Schnitt-Lupe", "Precision Cut Magnifier"), |ui| {
            // Row 1: Marker selector + Timecode editor + Fine Nudge buttons + Delete (fits on 1 line in 480px)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);
                if let Some((m_idx, m_time)) = active_marker {
                    if ui.add_enabled(m_idx > 0, egui::Button::new("◀").small())
                        .on_hover_text(self.text("Vorherige Schnittmarke", "Previous marker"))
                        .clicked()
                    {
                        self.select_marker(m_idx - 1);
                    }
                    ui.label(
                        RichText::new(format!("T{:02}({}/{})", m_idx + 2, m_idx + 1, marker_count))
                            .strong()
                            .monospace()
                            .small()
                            .color(LED_AMBER),
                    );
                    if ui.add_enabled(m_idx + 1 < marker_count, egui::Button::new("▶").small())
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
                        if ui.small_button(label)
                            .on_hover_text(self.text("Schnittmarke fein verschieben (Pfeiltasten ←/→)", "Fine-nudge cut marker (Arrow keys ←/→)"))
                            .clicked()
                        {
                            self.nudge_marker(m_idx, delta, total_sec);
                        }
                    }

                    if ui.small_button("🗑")
                        .on_hover_text(self.text("Schnittmarke entfernen (⌫)", "Delete cut marker (⌫)"))
                        .clicked()
                    {
                        self.request_marker_delete(m_idx);
                    }
                } else {
                    ui.label(RichText::new(self.text("Klicke in die Lupe oder drücke M", "Click inside scope or press M")).color(TEXT_SECONDARY).small());
                    if ui.small_button(self.text("│ Marke setzen", "│ Place marker")).clicked() {
                        self.add_marker(cur_sec);
                    }
                }
            });

            // Row 2: Window radius pills + Auto-Gain + Transients (max 3) & sensitivity (fits on 1 line in 480px)
            ui.horizontal(|ui| {
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
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding = Vec2::new(6.0, 2.0);
                let ref_t = active_marker.map_or(cur_sec, |(_, t)| t);
                if ui.small_button(self.text("▶ Davor 2s", "▶ Before 2s"))
                    .on_hover_text(self.text("Spielt die letzten 2 Sekunden vor dem Schnitt (Ende des vorherigen Tracks)", "Plays 2 seconds before the cut"))
                    .clicked()
                {
                    self.player.play_range((ref_t - 2.0).max(0.0), ref_t);
                }
                if ui.small_button(self.text("▶ Danach 2s", "▶ After 2s"))
                    .on_hover_text(self.text("Spielt die ersten 2 Sekunden ab dem Schnitt (Start des neuen Tracks)", "Plays 2 seconds starting at the cut"))
                    .clicked()
                {
                    self.player.play_range(ref_t, (ref_t + 2.0).min(total_sec));
                }
                if ui.small_button(self.text("▶ Übergang", "▶ Transition"))
                    .on_hover_text(self.text("Spielt über die Schnittgrenze hinweg (±1.5s)", "Plays across the cut boundary (±1.5s)"))
                    .clicked()
                {
                    self.player.play_range((ref_t - 1.5).max(0.0), (ref_t + 1.5).min(total_sec));
                }
                if ui.small_button(self.text("🔇 Rauschfilter & Schnitt", "🔇 Denoise & Cut"))
                    .on_hover_text(self.text("Wechselt zum Rauschfilter & Originalschnitt-Studio (Taste F)", "Switch to Denoise & Original Cut Studio (Key F)"))
                    .clicked()
                {
                    self.denoiser_panel_active = true;
                    self.magnifier_scope_active = false;
                    self.refresh_noise_presets();
                }

                if self.show_transients && !transients.is_empty() {
                    ui.separator();
                    for (t_i, tp) in transients.iter().take(crate::splitter::MAX_MARKER_TRANSIENTS).enumerate() {
                        let delta = tp.time_sec - ref_t;
                        let btn_text = format!("⚡{}{:+.2}s", t_i + 1, delta);
                        if ui.small_button(RichText::new(btn_text).monospace().small().color(Color32::from_rgb(255, 195, 40)))
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
                            self.player.play_range(target, (target + 1.5).min(total_sec));
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
                self.last_saved_bytes_info = Some(
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
                self.last_saved_bytes_info = Some(format!(
                    "✓ Preset gespeichert (App & Logic AU): {}",
                    wav_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(err) => self.ui_error_message = Some(err),
        }
    }

    fn cut_or_trim_original_recording(&mut self, ctx: &egui::Context, crop_to_selection: bool) {
        let Some((r_start, r_end)) = self.selected_region else {
            self.ui_error_message = Some(
                self.text(
                    "Bitte markiere zuerst mit 'Bereich·R' einen Abschnitt in der Wellenform.",
                    "Please select a time range on the waveform using 'Range·R' first.",
                )
                .into(),
            );
            return;
        };
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let sr = self.player.sample_rate();
        let mut markers = self.splitter.snapshot();
        let audio_path = self.current_audio_path.clone();
        let overwrite = self.auto_overwrite_original_wav;
        let is_de = self.language == Language::De;

        self.player.pause();
        let label = if crop_to_selection {
            self.text("Aufnahme wird auf Auswahl getrimmt …", "Trimming recording to selection …")
        } else {
            self.text("Auswahl wird aus Aufnahme geschnitten …", "Cutting selection from recording …")
        };

        self.start_job(ctx, label, move || {
            let old_bytes = audio_path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len());

            let (new_l, new_r) = if crop_to_selection {
                crop_trim_audio_to_range(&left, &right, sr, r_start, r_end, &mut markers)?
            } else {
                cut_delete_audio_range(&left, &right, sr, r_start, r_end, &mut markers)?
            };

            let status_msg = if overwrite {
                if let Some(ref target) = audio_path {
                    write_stereo_wav_atomic(target, &new_l, &new_r, sr)?;
                    let new_bytes = std::fs::metadata(target).ok().map(|m| m.len());
                    if let (Some(old_b), Some(new_b)) = (old_bytes, new_bytes) {
                        let saved_mb = (old_b.saturating_sub(new_b) as f64) / (1024.0 * 1024.0);
                        Some(if is_de {
                            format!("✓ Original-WAV ohne Backup überschrieben (−{saved_mb:.1} MB Speicherplatz gespart)")
                        } else {
                            format!("✓ Overwrote original WAV without backup (−{saved_mb:.1} MB disk space saved)")
                        })
                    } else {
                        Some(if is_de {
                            "✓ Original-WAV direkt überschrieben".into()
                        } else {
                            "✓ Overwrote original WAV directly".into()
                        })
                    }
                } else {
                    None
                }
            } else {
                Some(if is_de {
                    "✓ Schnitt im Speicher angewendet (zum Sichern 'WAV speichern' klicken)".into()
                } else {
                    "✓ Applied cut in memory (click 'Save WAV' to write to disk)".into()
                })
            };

            let decoded = DecodedAudio {
                left: new_l,
                right: new_r,
                sample_rate: sr,
            };
            let waveform = WaveformCache::build(&decoded);
            let report = analyze_samples(&decoded.left, &decoded.right, sr)?;
            Ok(JobResult::AudioModified {
                audio: decoded,
                waveform,
                report,
                markers,
                status_msg,
                disable_live_denoiser: false,
            })
        });
    }

    fn bake_denoiser_into_recording(&mut self, ctx: &egui::Context) {
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let sr = self.player.sample_rate();
        if left.is_empty() || sr == 0 {
            return;
        }
        let markers = self.splitter.snapshot();
        let profile = self.active_noise_profile.clone();
        let mut params = self.denoiser_params.clone();
        params.enabled = true;
        params.audition_mode = AuditionMode::Wet;
        let audio_path = self.current_audio_path.clone();
        let overwrite = self.auto_overwrite_original_wav;
        let is_de = self.language == Language::De;

        self.player.pause();
        self.start_job(
            ctx,
            self.text(
                "Rausch- & DC-Filter wird in Aufnahme eingerechnet …",
                "Applying noise & DC filter to recording …",
            ),
            move || {
                let (clean_l, clean_r) =
                    SpectralDenoiser::process_offline_stereo(&left, &right, sr, profile, &params);
                let status_msg = if overwrite {
                    if let Some(ref target) = audio_path {
                        write_stereo_wav_atomic(target, &clean_l, &clean_r, sr)?;
                        Some(if is_de {
                            "✓ Aufnahme entrauscht & Original-WAV direkt aktualisiert".into()
                        } else {
                            "✓ Denoised recording & updated original WAV directly".into()
                        })
                    } else {
                        None
                    }
                } else {
                    Some(if is_de {
                        "✓ Filter in Aufnahme eingerechnet".into()
                    } else {
                        "✓ Baked filter into recording".into()
                    })
                };
                let decoded = DecodedAudio {
                    left: clean_l,
                    right: clean_r,
                    sample_rate: sr,
                };
                let waveform = WaveformCache::build(&decoded);
                let report = analyze_samples(&decoded.left, &decoded.right, sr)?;
                Ok(JobResult::AudioModified {
                    audio: decoded,
                    waveform,
                    report,
                    markers,
                    status_msg,
                    disable_live_denoiser: true,
                })
            },
        );
    }

    fn bake_denoiser_into_track(&mut self, ctx: &egui::Context, segment_idx: usize) {
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let sr = self.player.sample_rate();
        let total_sec = self.player.total_seconds();
        if left.is_empty() || sr == 0 || total_sec <= 0.0 {
            return;
        }
        let tracks = self.splitter.get_tracks(total_sec);
        let Some(track) = tracks.iter().find(|t| t.segment_index == segment_idx).cloned() else {
            return;
        };

        let s0 = (track.start_seconds * sr as f64).round() as usize;
        let s1 = ((track.end_seconds * sr as f64).round() as usize).min(left.len());
        if s0 >= s1 || s0 >= left.len() {
            return;
        }

        let markers = self.splitter.snapshot();
        let profile = self.active_noise_profile.clone();
        let mut params = self.denoiser_params.clone();
        params.enabled = true;
        params.audition_mode = AuditionMode::Wet;
        let audio_path = self.current_audio_path.clone();
        let overwrite = self.auto_overwrite_original_wav;
        let is_de = self.language == Language::De;
        let track_name = track.title.clone();
        let track_idx = track.index;

        self.player.pause();
        let job_lbl = if is_de {
            "Rauschfilter wird in Track eingerechnet …"
        } else {
            "Applying noise filter to track …"
        };
        self.start_job(
            ctx,
            job_lbl,
            move || {
                let track_slice_l = &left[s0..s1];
                let track_slice_r = &right[s0..s1];
                let (mut clean_l, mut clean_r) =
                    SpectralDenoiser::process_offline_stereo(track_slice_l, track_slice_r, sr, profile, &params);

                // Crossfade boundary samples (5ms) to prevent clicks
                let xfade = ((sr as f64 * 0.005).round() as usize).min(clean_l.len() / 4);
                if xfade > 0 {
                    for i in 0..xfade {
                        let w = i as f32 / xfade as f32;
                        clean_l[i] = track_slice_l[i] * (1.0 - w) + clean_l[i] * w;
                        clean_r[i] = track_slice_r[i] * (1.0 - w) + clean_r[i] * w;

                        let end_idx = clean_l.len() - 1 - i;
                        clean_l[end_idx] = track_slice_l[end_idx] * (1.0 - w) + clean_l[end_idx] * w;
                        clean_r[end_idx] = track_slice_r[end_idx] * (1.0 - w) + clean_r[end_idx] * w;
                    }
                }

                let mut out_l = (*left).clone();
                let mut out_r = (*right).clone();
                out_l[s0..s1].copy_from_slice(&clean_l);
                out_r[s0..s1].copy_from_slice(&clean_r);

                let status_msg = if overwrite {
                    if let Some(ref target) = audio_path {
                        write_stereo_wav_atomic(target, &out_l, &out_r, sr)?;
                        Some(if is_de {
                            format!("✓ Track {:02} („{}“) entrauscht & Original-WAV aktualisiert", track_idx, track_name)
                        } else {
                            format!("✓ Track {:02} (\"{}\") denoised & updated original WAV", track_idx, track_name)
                        })
                    } else {
                        None
                    }
                } else {
                    Some(if is_de {
                        format!("✓ Filter in Track {:02} („{}“) eingerechnet", track_idx, track_name)
                    } else {
                        format!("✓ Baked filter into Track {:02} (\"{}\")", track_idx, track_name)
                    })
                };

                let decoded = DecodedAudio {
                    left: out_l,
                    right: out_r,
                    sample_rate: sr,
                };
                let waveform = WaveformCache::build(&decoded);
                let report = analyze_samples(&decoded.left, &decoded.right, sr)?;
                Ok(JobResult::AudioModified {
                    audio: decoded,
                    waveform,
                    report,
                    markers,
                    status_msg,
                    disable_live_denoiser: false,
                })
            },
        );
    }

    fn overwrite_original_wav_now(&mut self, ctx: &egui::Context) {
        let Some(target) = self.current_audio_path.clone() else {
            return;
        };
        let left = self.player.get_samples_l();
        let right = self.player.get_samples_r();
        let sr = self.player.sample_rate();
        let markers = self.splitter.snapshot();
        let is_de = self.language == Language::De;

        self.start_job(
            ctx,
            self.text("Original-WAV wird überschrieben …", "Overwriting original WAV …"),
            move || {
                let old_bytes = std::fs::metadata(&target).ok().map(|m| m.len());
                write_stereo_wav_atomic(&target, &left, &right, sr)?;
                let new_bytes = std::fs::metadata(&target).ok().map(|m| m.len());
                let status_msg = if let (Some(old_b), Some(new_b)) = (old_bytes, new_bytes) {
                    let saved_mb = (old_b.saturating_sub(new_b) as f64) / (1024.0 * 1024.0);
                    Some(if is_de {
                        format!("✓ Original-WAV gespeichert (−{saved_mb:.1} MB)")
                    } else {
                        format!("✓ Saved original WAV (−{saved_mb:.1} MB)")
                    })
                } else {
                    Some(if is_de { "✓ Original-WAV gespeichert".into() } else { "✓ Saved original WAV".into() })
                };
                let decoded = DecodedAudio {
                    left: (*left).clone(),
                    right: (*right).clone(),
                    sample_rate: sr,
                };
                let waveform = WaveformCache::build(&decoded);
                let report = analyze_samples(&decoded.left, &decoded.right, sr)?;
                Ok(JobResult::AudioModified {
                    audio: decoded,
                    waveform,
                    report,
                    markers,
                    status_msg,
                    disable_live_denoiser: false,
                })
            },
        );
    }

    fn render_denoiser_and_cut_panel(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64) {
        let (sel_start, sel_end) = self.active_selection_or_window(total_sec, cur_sec);
        let sel_dur = (sel_end - sel_start).max(0.0);

        studio_panel(
            ui,
            self.text(
                "Rausch- & DC-Filter-Studio",
                "Noise & DC Filter Studio",
            ),
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;

                // -------------------------------------------------------------
                // ROW 1: ORIGINALE AUFNAHME SCHNEIDEN (OHNE BACKUP SPEICHERN)
                // -------------------------------------------------------------
                ui.horizontal(|ui| {
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
                    ui.label(RichText::new("–").color(TEXT_MUTED).small());
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

                    if ui
                        .small_button(format!("▶{:.1}s", sel_dur))
                        .on_hover_text(self.text(
                            "Ausgewählten Bereich vorhören",
                            "Audition selected range",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.player.play_range(sel_start, sel_end);
                    }

                    if ui
                        .add_enabled(
                            sel_dur >= 0.05,
                            egui::Button::new(
                                RichText::new(self.text("✂ Löschen", "✂ Delete")).color(LED_AMBER).small(),
                            )
                            .small(),
                        )
                        .on_hover_text(self.text(
                            "Schneidet den markierten Bereich aus der Originalaufnahme heraus (inkl. 5ms Crossfade & Marker-Korrektur)",
                            "Cuts the selected range out of the original recording (with 5ms crossfade & marker shift)",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.cut_or_trim_original_recording(ui.ctx(), false);
                    }

                    if ui
                        .add_enabled(
                            sel_dur >= 0.1 && sel_dur < total_sec - 0.05,
                            egui::Button::new(RichText::new(self.text("⬚ Trim", "⬚ Crop")).small()).small(),
                        )
                        .on_hover_text(self.text(
                            "Behält nur den markierten Bereich und entfernt Vor- und Nachlauf der Aufnahme",
                            "Keeps only the selected range and removes everything before and after",
                        ))
                        .clicked()
                    {
                        self.selected_region = Some((sel_start, sel_end));
                        self.cut_or_trim_original_recording(ui.ctx(), true);
                    }

                    let overwrite_lbl = self.text("WAV direkt", "Direct WAV");
                    let overwrite_tip = self.text(
                        "Überschreibt die Original-WAV direkt ohne Sicherheitskopie, um Speicherplatz zu sparen",
                        "Overwrites the original WAV directly without backup to save disk space",
                    );
                    ui.checkbox(
                        &mut self.auto_overwrite_original_wav,
                        RichText::new(overwrite_lbl).small(),
                    )
                    .on_hover_text(overwrite_tip);

                    if !self.auto_overwrite_original_wav && self.current_audio_path.is_some() {
                        if ui
                            .small_button("💾")
                            .on_hover_text(self.text("Original-WAV jetzt überschreiben", "Overwrite original WAV now"))
                            .clicked()
                        {
                            self.overwrite_original_wav_now(ui.ctx());
                        }
                    }

                    if ui
                        .small_button("🔍")
                        .on_hover_text(self.text("Zur Schnitt-Lupe wechseln (Taste L)", "Switch to Cut Magnifier (Key L)"))
                        .clicked()
                    {
                        self.magnifier_scope_active = true;
                        self.denoiser_panel_active = false;
                    }
                });

                // -------------------------------------------------------------
                // ROW 2: RAUSCH-SAMPLE LERNEN & PRESET-VERWALTUNG (WAV + JSON)
                // -------------------------------------------------------------
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);

                    let learn_lbl = self.text("🎯 Rauschen lernen", "🎯 Learn Noise");
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(learn_lbl).color(Color32::BLACK).strong().small(),
                            )
                            .fill(ACCENT)
                            .small(),
                        )
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
                            egui::Button::new(RichText::new(self.text("💾 Preset", "💾 Save")).small()).small(),
                        )
                        .on_hover_text(self.text(
                            "Speichert die Rausch-Samples als WAV + Spektralprofil für andere Aufnahmen & das Logic Pro AudioUnit",
                            "Saves noise samples as WAV + spectral profile for other recordings & Logic Pro AudioUnit",
                        ))
                        .clicked()
                    {
                        self.save_current_noise_preset();
                    }

                    if ui
                        .small_button("📂")
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
                        if ui
                            .small_button("🗑")
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
                ui.horizontal(|ui| {
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

                    let bake_btn_lbl = self.text("✨ Einrechnen ▼", "✨ Apply ▼");
                    let bake_tip = self.text(
                        "Filter permanent einrechnen: Klick für Optionen (nur gewählter Track oder ganze Aufnahme)",
                        "Bake filter permanently: Click for options (selected track only or entire recording)",
                    );
                    ui.add_enabled_ui(can_apply, |ui| {
                        ui.menu_button(RichText::new(bake_btn_lbl).color(LED_GREEN).strong().small(), |ui| {
                            if let Some(ref track) = selected_track {
                                let track_seg = track.segment_index;
                                let track_txt = if self.language == Language::De {
                                    format!("Track {:02} filtern („{}“)", track.index, track.title)
                                } else {
                                    format!("Filter Track {:02} (\"{}\")", track.index, track.title)
                                };
                                if ui.button(track_txt).clicked() {
                                    self.bake_denoiser_into_track(ui.ctx(), track_seg);
                                    ui.close_menu();
                                }
                            }
                            let all_txt = self.text("Ganze Aufnahme filtern", "Filter entire recording");
                            if ui.button(all_txt).clicked() {
                                self.bake_denoiser_into_recording(ui.ctx());
                                ui.close_menu();
                            }
                        }).response.on_hover_text(bake_tip);
                    });
                });

                if params_changed {
                    self.player.set_denoiser_params(self.denoiser_params.clone());
                }

                // -------------------------------------------------------------
                // ROW 4: SPEKTRALES RAUSCHPROFIL-DISPLAY & STATUS
                // -------------------------------------------------------------
                let desired_h = (ui.available_height() - 4.0).clamp(24.0, 96.0);
                let (spec_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), desired_h), egui::Sense::hover());
                let painter = ui.painter();
                draw_recessed_well(painter, spec_rect, 4.0, BG_INSET);

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
                            Color32::from_rgba_unmultiplied(56, 189, 248, 175)
                        } else {
                            Color32::from_rgba_unmultiplied(120, 130, 150, 110)
                        };
                        painter.rect_filled(bar_rect, 1.5, col);
                    }

                    let dc_pct = (prof.dc_offset_l.abs().max(prof.dc_offset_r.abs())) * 100.0;
                    let info = if let Some(ref msg) = self.last_saved_bytes_info {
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
                        LED_GREEN,
                    );
                } else {
                    let hint = self.last_saved_bytes_info.as_deref().unwrap_or_else(|| {
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
                        if self.last_saved_bytes_info.is_some() { LED_GREEN } else { TEXT_SECONDARY },
                    );
                }
            },
        );
    }

    fn render_track_list(&mut self, ui: &mut Ui, tracks: &[crate::splitter::TrackInfo]) {
        if !self.show_tracks {
            return;
        }
        if self.splitter.custom_titles.len() < tracks.len() {
            self.splitter.custom_titles.resize(tracks.len(), String::new());
        }

        let header_overhead = if self.batch_titles_open { 132.0 } else { 54.0 };
        let row_pitch = 33.0;
        let per_page = ((ui.available_height() - header_overhead) / row_pitch).floor().max(1.0) as usize;
        let pages = tracks.len().div_ceil(per_page).max(1);
        self.track_page = self.track_page.min(pages - 1);

        panel_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                panel_title(ui, self.text("Trackliste", "Track list"));
                let active_tracks = tracks.iter().filter(|t| !t.is_pause).count();
                let filtered_tracks = tracks.iter().filter(|t| !t.is_pause && t.is_filtered).count();
                ui.label(RichText::new(format!("{active_tracks} Tracks")).color(TEXT_SECONDARY).small());

                let filter_menu_lbl = format!("🔇 Filter ({filtered_tracks}/{active_tracks})");
                ui.menu_button(&filter_menu_lbl, |ui| {
                    if ui.button(self.text("Alle Tracks filtern", "Filter all tracks")).clicked() {
                        self.splitter.set_all_tracks_filtered(tracks.len(), true);
                        ui.close_menu();
                    }
                    if ui.button(self.text("Keine Tracks filtern", "Filter no tracks")).clicked() {
                        self.splitter.set_all_tracks_filtered(tracks.len(), false);
                        ui.close_menu();
                    }
                    if let Some(seg_idx) = self.selected_segment_idx {
                        if let Some(t) = tracks.iter().find(|t| t.segment_index == seg_idx && !t.is_pause) {
                            ui.separator();
                            let toggle_lbl = if self.language == Language::De {
                                if t.is_filtered {
                                    format!("Track {:02} nicht filtern", t.index)
                                } else {
                                    format!("Track {:02} filtern", t.index)
                                }
                            } else if t.is_filtered {
                                format!("Don't filter Track {:02}", t.index)
                            } else {
                                format!("Filter Track {:02}", t.index)
                            };
                            if ui.button(toggle_lbl).clicked() {
                                self.splitter.toggle_track_filter(t.segment_index);
                                ui.close_menu();
                            }
                            if self.active_noise_profile.is_some() || self.denoiser_params.dc_filter_enabled {
                                let bake_lbl = if self.language == Language::De {
                                    format!("✨ Track {:02} jetzt filtern (einrechnen)", t.index)
                                } else {
                                    format!("✨ Bake filter into Track {:02}", t.index)
                                };
                                if ui.button(bake_lbl).clicked() {
                                    self.bake_denoiser_into_track(ui.ctx(), t.segment_index);
                                    ui.close_menu();
                                }
                            }
                        }
                    }
                });

                if ui.small_button(self.text("📋 Liste einfügen", "📋 Paste list"))
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
                        if ui.button(apply_txt).clicked() {
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
                        if ui.small_button(cancel_txt).clicked() {
                            self.batch_titles_open = false;
                        }
                    });
                });
            }

            ui.add_space(4.0);
            let start = self.track_page * per_page;
            for (index, track) in tracks.iter().enumerate().skip(start).take(per_page) {
                let selected = self.selected_segment_idx == Some(index);
                let border = if selected { ACCENT } else { BORDER_MID };
                let row_bg = if selected {
                    Color32::from_rgb(28, 32, 42)
                } else {
                    BG_WINDOW
                };

                egui::Frame::none()
                    .fill(row_bg)
                    .stroke(Stroke::new(1.0, border))
                    .rounding(6.0)
                    .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.spacing_mut().button_padding = Vec2::new(5.0, 2.0);
                            let badge_txt = if track.is_pause { "⏸".into() } else { format!("{:02}", track.index) };
                            let badge = egui::Button::new(RichText::new(badge_txt).monospace().small()).selected(selected);
                            if ui.add_sized([26.0, 20.0], badge).clicked() {
                                self.selected_segment_idx = Some(index);
                                self.selected_marker_idx = index.checked_sub(1);
                                self.player.seek_seconds(track.start_seconds);
                            }

                            // Reserve exact space for the compact right-hand controls so the Track Title field
                            // is always >= 20 characters wide (>= 180 pt in split view, >= 520 pt in full view)
                            // while never overflowing the row width.
                            let right_controls_w = if index > 0 { 210.0 } else { 188.0 };
                            let title_width = (ui.available_width() - right_controls_w).max(150.0);

                            if track.is_pause {
                                ui.add_sized(
                                    [title_width, 20.0],
                                    egui::Label::new(RichText::new(self.language.badge_pause()).color(LED_AMBER).small()),
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
                                    self.magnifier_scope_active = true;
                                }
                            } else {
                                ui.label(start_tc.color(TEXT_SECONDARY));
                            }

                            ui.label(RichText::new(track.duration_formatted()).monospace().small().color(TEXT_SECONDARY));

                            if ui.small_button("▶")
                                .on_hover_text(self.text("Track anhören", "Audition track"))
                                .clicked()
                            {
                                self.player.play_range(track.start_seconds, track.end_seconds);
                            }

                            if !track.is_pause {
                                let flt_color = if track.is_filtered { LED_GREEN } else { TEXT_MUTED };
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
                                let btn_resp = ui.small_button(RichText::new("🔇").color(flt_color)).on_hover_text(flt_tip);
                                if btn_resp.clicked() {
                                    self.splitter.toggle_track_filter(track.segment_index);
                                }
                                btn_resp.context_menu(|ui| {
                                    let tog_txt = if track.is_filtered {
                                        self.text("Rauschfilter für diesen Track deaktivieren", "Disable noise filter for this track")
                                    } else {
                                        self.text("Rauschfilter für diesen Track aktivieren", "Enable noise filter for this track")
                                    };
                                    if ui.button(tog_txt).clicked() {
                                        self.splitter.toggle_track_filter(track.segment_index);
                                        ui.close_menu();
                                    }
                                    if self.active_noise_profile.is_some() || self.denoiser_params.dc_filter_enabled {
                                        if ui.button(self.text("✨ Filter jetzt in diesen Track einrechnen", "✨ Bake filter into this track now")).clicked() {
                                            self.bake_denoiser_into_track(ui.ctx(), track.segment_index);
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }

                            let pause_icon = if track.is_pause { "↩" } else { "⏸" };
                            let pause_tip = if track.is_pause {
                                self.language.btn_make_track()
                            } else {
                                self.language.btn_make_pause()
                            };
                            if ui.small_button(pause_icon).on_hover_text(pause_tip).clicked() {
                                self.splitter.toggle_pause(index);
                            }

                            if index > 0 && ui.small_button("×")
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
                    self.player.audition_at(time);
                    self.hover_audition_time = Some(time);
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

    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        let progress = self.job.as_ref().and_then(|job| job.export_progress.clone());
        if progress.is_none() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.show_export = false;
            self.export_outcome = None;
            return;
        }

        let screen = ctx.screen_rect();
        ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("export_backdrop")))
            .rect_filled(screen, 0.0, Color32::from_black_alpha(144));
        let width = (screen.width() - 48.0).min(640.0);
        egui::Window::new("Track export")
            .id(egui::Id::new("export_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false).resizable(false)
            .default_width(width).default_height(0.0)
            .frame(egui::Frame::window(&ctx.style()).inner_margin(18.0).rounding(14.0))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.heading(self.text("Tracks exportieren", "Export tracks"));
                ui.add_space(2.0);
                let sub = if self.artist_name.trim().is_empty() {
                    format!("{}  ·  {}", self.tape_name, self.tape_side.label(self.language))
                } else {
                    format!("{} – {}  ·  {}", self.artist_name.trim(), self.tape_name, self.tape_side.label(self.language))
                };
                ui.label(RichText::new(sub).color(TEXT_SECONDARY));
                ui.add_space(8.0);

                if let Some(progress) = progress {
                    let done = progress.finished.load(Ordering::Relaxed);
                    let total = progress.total.load(Ordering::Relaxed);
                    let cancelling = progress.cancelled.load(Ordering::Relaxed);
                    ui.label(if cancelling {
                        self.text("Export wird abgebrochen …", "Cancelling export …")
                    } else {
                        self.text("WAV-Dateien werden geschrieben …", "Writing WAV files …")
                    });
                    ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .text(format!("{done} / {total}")).animate(true));
                    ui.add_space(12.0);
                    if ui.add_enabled(!cancelling, egui::Button::new(self.language.btn_cancel())).clicked() {
                        progress.cancelled.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(Duration::from_millis(150));
                    return;
                }

                if let Some(ExportOutcome::Done { count, folder }) = &self.export_outcome {
                    ui.label(RichText::new(format!("✓  {} {count}", self.text("Exportierte Tracks", "Exported tracks")))
                        .strong().color(LED_GREEN));
                    let path = folder.display().to_string();
                    ui.add(egui::Label::new(&path).truncate()).on_hover_text(path);
                    ui.add_space(16.0);
                    let done_label = self.text("Fertig", "Done");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(done_label).clicked() {
                            self.show_export = false;
                        }
                        if ui.button(self.language.btn_show_in_finder()).clicked() {
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(folder).spawn();
                        }
                    });
                    if !self.show_export {
                        self.export_outcome = None;
                    }
                    return;
                }

                let tracks = self.splitter.get_tracks(self.player.total_seconds());
                let target = self.effective_export_directory();
                let artist_prefix = sanitize_filename(self.artist_name.trim());
                let has_artist = !self.artist_name.trim().is_empty();
                let mut files = Vec::new();
                for track in tracks.iter().filter(|track| !track.is_pause) {
                    let name = if has_artist {
                        format!("{:02}_{}_{}.wav", track.index, artist_prefix, sanitize_filename(&track.title))
                    } else {
                        track.export_filename()
                    };
                    let exists = target.join(&name).exists();
                    files.push((track, name, exists));
                }
                let count = files.len();
                let conflicts = files.iter().filter(|(_, _, exists)| *exists).count();
                let export_enabled = count > 0 && conflicts == 0 && self.current_audio_path.is_some()
                    && self.job.is_none() && !self.recorder.is_busy();

                ui.horizontal_wrapped(|ui| {
                    panel_title(ui, self.text("Zielordner", "Destination"));
                    if ui.button(self.text("Basisordner ändern …", "Change base folder …")).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().set_directory(&self.export_directory).pick_folder() {
                            self.export_directory = folder;
                            self.export_outcome = None;
                            self.save_config();
                            ctx.request_repaint();
                        }
                    }
                });
                let target_label = self.effective_export_directory().display().to_string();
                ui.add(egui::Label::new(RichText::new(&target_label).color(TEXT_SECONDARY)).truncate())
                    .on_hover_text(target_label);

                ui.add_space(8.0);
                panel_title(ui, self.text("Audio & Analog-Mastering", "Audio & Analog Mastering"));
                ui.label(RichText::new(format!("WAV  ·  PCM 24-bit  ·  Stereo  ·  {} Hz", self.player.sample_rate()))
                    .color(TEXT_SECONDARY).small());
                let mode_label = |mode: &NormalizationMode| match mode {
                    NormalizationMode::None => self.text("Originalpegel", "Original level"),
                    NormalizationMode::PeakPerTrack => self.text("Peak pro Track · −0,3 dBFS", "Peak per track · −0.3 dBFS"),
                    NormalizationMode::PeakAlbum => self.text("Einheitlicher Pegel für das Album", "Shared gain across the album"),
                };
                let previous = self.normalization_mode.clone();
                let options = [NormalizationMode::None, NormalizationMode::PeakPerTrack, NormalizationMode::PeakAlbum];
                let labels = options.each_ref().map(mode_label);
                egui::ComboBox::from_label(self.text("Normalisierung", "Normalization"))
                    .selected_text(mode_label(&previous))
                    .show_ui(ui, |ui| {
                        for (mode, label) in options.into_iter().zip(labels) {
                            ui.selectable_value(&mut self.normalization_mode, mode, label);
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
                    ui.label(RichText::new(self.text("Schnitt-Fade:", "Cut fade:")).color(TEXT_SECONDARY).small());
                    for (ms, lbl) in [(0_u32, "Aus"), (15, "15 ms"), (100, "100 ms"), (300, "300 ms")] {
                        if ui.selectable_label(self.export_fade_ms == ms, lbl).clicked() {
                            self.export_fade_ms = ms;
                            self.save_config();
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("L/R Trim:").color(TEXT_SECONDARY).small());
                    ui.add(egui::DragValue::new(&mut self.export_balance_db).range(-12.0..=12.0).speed(0.1).suffix(" dB"));
                    if self.active_noise_profile.is_some() {
                        ui.separator();
                        let nr_lbl = self.text("Rauschfilter anwenden", "Apply noise filter");
                        ui.checkbox(&mut self.export_apply_denoiser, nr_lbl);
                        if self.export_apply_denoiser {
                            let filtered_tracks_count = tracks.iter().filter(|t| !t.is_pause && t.is_filtered).count();
                            let total_active_tracks = tracks.iter().filter(|t| !t.is_pause).count();
                            ui.label(RichText::new(format!("({filtered_tracks_count}/{total_active_tracks} Tracks)")).color(TEXT_SECONDARY).small());
                            if ui.small_button(self.text("Alle", "All")).on_hover_text(self.text("Filter für alle Tracks aktivieren", "Enable filter for all tracks")).clicked() {
                                self.splitter.set_all_tracks_filtered(tracks.len(), true);
                            }
                            if ui.small_button(self.text("Keine", "None")).on_hover_text(self.text("Filter für alle Tracks deaktivieren", "Disable filter for all tracks")).clicked() {
                                self.splitter.set_all_tracks_filtered(tracks.len(), false);
                            }
                        }
                    }
                });

                ui.add_space(8.0);
                let per_page = ((screen.height() - 450.0) / 28.0).floor().clamp(2.0, 8.0) as usize;
                let pages = count.div_ceil(per_page).max(1);
                self.export_page = self.export_page.min(pages - 1);
                ui.horizontal(|ui| {
                    panel_title(ui, &format!("{} {count}", self.text("Dateien", "Files")));
                    page_controls(ui, &mut self.export_page, pages, self.language);
                });
                let gaps = tracks.len() - count;
                if gaps > 0 {
                    ui.label(RichText::new(format!("{} {gaps}", self.text("Ausgelassene Pausen", "Excluded gaps")))
                        .color(TEXT_SECONDARY).small());
                }
                if count == 0 {
                    ui.label(self.text("Keine Tracks zum Exportieren.", "No tracks to export."));
                }
                for (track, name, exists) in files.iter().skip(self.export_page * per_page).take(per_page) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.monospace(track.duration_formatted());
                        if self.export_apply_denoiser && self.active_noise_profile.is_some() {
                            let (flt_lbl, flt_col, flt_tip) = if track.is_filtered {
                                (
                                    self.text("🔇 Filter", "🔇 Filter"),
                                    LED_GREEN,
                                    self.text("Dieser Track wird entrauscht. Klick zum Deaktivieren.", "This track will be denoised. Click to disable."),
                                )
                            } else {
                                (
                                    self.text("🔈 Bypass", "🔈 Bypass"),
                                    TEXT_MUTED,
                                    self.text("Dieser Track wird ungefiltert exportiert. Klick zum Aktivieren.", "This track will be exported unfiltered. Click to enable."),
                                )
                            };
                            if ui.small_button(RichText::new(flt_lbl).color(flt_col).small()).on_hover_text(flt_tip).clicked() {
                                self.splitter.toggle_track_filter(track.segment_index);
                            }
                        }
                        if *exists {
                            ui.colored_label(LED_AMBER, self.text("Existiert bereits", "Already exists"));
                        }
                        ui.add(egui::Label::new(name).truncate()).on_hover_text(name);
                    });
                }
                if conflicts > 0 {
                    ui.colored_label(LED_AMBER, self.text(
                        "Wähle einen anderen Basisordner oder räume den Zielordner frei.",
                        "Choose a different base folder or clear the conflicting files.",
                    ));
                }
                match &self.export_outcome {
                    Some(ExportOutcome::Failed(error)) => { ui.add(egui::Label::new(RichText::new(error).color(LED_RED)).truncate()).on_hover_text(error); }
                    Some(ExportOutcome::Cancelled) => {
                        ui.label(self.text("Export abgebrochen.", "Export cancelled."));
                    }
                    _ => {}
                }
                ui.add_space(8.0);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = self.text("Exportieren", "Export");
                    let export = primary_button(label);
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
                            self.start_job(ctx, self.text("Tracks werden exportiert …", "Exporting tracks …"), move || {
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
                    if ui.button(self.language.btn_cancel()).clicked() {
                        self.show_export = false;
                        self.export_outcome = None;
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
        if ui.add_enabled(*page + 1 < pages, egui::Button::new("›")).on_hover_text(next).clicked() {
            *page += 1;
        }
        ui.label(RichText::new(format!("{} / {pages}", *page + 1)).monospace());
        if ui.add_enabled(*page > 0, egui::Button::new("‹")).on_hover_text(previous).clicked() {
            *page -= 1;
        }
    });
}

fn render_hardware_metric(ui: &mut Ui, width: f32, label: &str, value: &str, val_col: Color32) {
    let desired_size = Vec2::new(width, 60.0);
    let (rect, _resp) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter();

    draw_recessed_well(painter, rect, 4.0, BG_INSET);

    painter.text(
        Pos2::new(rect.min.x + 10.0, rect.min.y + 10.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(9.5),
        TEXT_MUTED,
    );

    painter.text(
        Pos2::new(rect.min.x + 10.0, rect.min.y + 28.0),
        egui::Align2::LEFT_TOP,
        value,
        egui::FontId::monospace(16.0),
        val_col,
    );
}

fn dirs_default_music_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let music = PathBuf::from(home).join("Music").join("TapeDigizer");
        Some(music)
    } else {
        None
    }
}

#[derive(Default)]
struct WaveformCache {
    // Finest to coarsest, with uniform time bins.
    levels: Vec<Vec<StereoWaveformPoint>>,
}

impl WaveformCache {
    fn build(audio: &DecodedAudio) -> Self {
        let frames = audio.left.len();
        if frames == 0 {
            return Self::default();
        }
        let bins = frames.min(65_536);
        let mut finest = Vec::with_capacity(bins);
        for index in 0..bins {
            let start = index * frames / bins;
            let end = (index + 1) * frames / bins;
            let mut point = StereoWaveformPoint::default();
            for (&l, &r) in audio.left[start..end].iter().zip(&audio.right[start..end]) {
                point.min_l = point.min_l.min(l);
                point.max_l = point.max_l.max(l);
                point.min_r = point.min_r.min(r);
                point.max_r = point.max_r.max(r);
            }
            finest.push(point);
        }
        let mut levels = vec![finest];
        while levels.last().is_some_and(|level| level.len() > 4096 && level.len() % 2 == 0) {
            let next = levels.last().unwrap().chunks_exact(2).map(|pair| StereoWaveformPoint {
                min_l: pair[0].min_l.min(pair[1].min_l),
                max_l: pair[0].max_l.max(pair[1].max_l),
                min_r: pair[0].min_r.min(pair[1].min_r),
                max_r: pair[0].max_r.max(pair[1].max_r),
            }).collect();
            levels.push(next);
        }
        Self { levels }
    }

    fn for_view(&self, zoom: f64) -> &[StereoWaveformPoint] {
        self.levels.iter().rev().find(|level| level.len() as f64 >= 2000.0 * zoom)
            .or_else(|| self.levels.first()).map_or(&[], Vec::as_slice)
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
                            &mut zoom, &mut vertical_zoom, 280.0, &mut scroll, EditTool::Pointer, false, None,
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

    #[test]
    fn test_render_tabs_without_recording() {
        let ctx = egui::Context::default();
        let mut app = TapeDigizerApp {
            language: Language::De,
            tape_name: "Test Tape".into(),
            artist_name: String::new(),
            media_source: MediaSourceType::Cassette,
            tape_side: TapeSide::SideA,
            current_project_path: None,
            current_audio_path: None,
            pending_delete_marker: None,
            active_tab: ActiveTab::Recording,
            available_devices: Vec::new(),
            selected_device_idx: 0,
            channel_pairs: Vec::new(),
            selected_pair_idx: 0,
            recorder: AudioRecorder::new(),
            player: AudioPlayer::new(),
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
            magnifier_scope_active: false,
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
            denoiser_panel_active: false,
            denoiser_params: DenoiserParams::default(),
            active_noise_profile: None,
            active_noise_samples: None,
            saved_noise_presets: Vec::new(),
            selected_preset_idx: None,
            preset_save_name: String::new(),
            auto_overwrite_original_wav: true,
            last_saved_bytes_info: None,
            export_apply_denoiser: true,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            show_tracks: true,
            batch_titles_open: false,
            batch_titles_text: String::new(),
            show_export: false,
            #[cfg(target_os = "macos")]
            mac_menu: None,
        };

        // 1. Run in Tab 1
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.render_tab_recording(ui);
            });
        });

        // 2. Switch to Tab 2 (Diagnosis) without recording
        app.active_tab = ActiveTab::Diagnosis;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.render_tab_diagnosis(ui);
            });
        });

        // 3. Switch to Tab 3 (Split & Export) without recording
        app.active_tab = ActiveTab::SplitAndExport;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.render_tab_split_export(ui);
            });
        });

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
                            egui::ScrollArea::vertical().show(ui, |ui| app.render_track_list(ui, &tracks));
                        });
                });
            }
            let field = ctx.read_response(egui::Id::new("track_title_field").with(0_usize)).unwrap();
            assert!(field.interact_rect.width() >= minimum_field_width,
                "Track title is only {} pt wide at {window_width} pt", field.interact_rect.width());
        }

        let audio = DecodedAudio { left: vec![0.1; 120_000], right: vec![0.2; 120_000], sample_rate: 1000 };
        app.waveform_cache = WaveformCache::build(&audio);
        app.analysis_report = Some(analyze_samples(&audio.left, &audio.right, audio.sample_rate).unwrap());
        app.player.load_test_samples(audio);
        app.splitter.markers = (1..10).map(|n| 10.0 * f64::from(n)).collect();
        app.select_marker(1);
        app.magnifier_scope_active = true;
        app.pause_preview = Some(app.splitter.snapshot());
        let workspace = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1060.0, 594.0));
        ctx.set_visuals(crate::ui::theme::studio_visuals());
        ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 7.0);
            style.spacing.button_padding = Vec2::new(9.0, 4.0);
        });
        for tab in [ActiveTab::Recording, ActiveTab::Diagnosis, ActiveTab::SplitAndExport] {
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput { screen_rect: Some(workspace), ..Default::default() }, |ctx| {
                    egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(16.0))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            match tab {
                                ActiveTab::Recording => app.render_tab_recording(ui),
                                ActiveTab::Diagnosis => app.render_tab_diagnosis(ui),
                                ActiveTab::SplitAndExport => app.render_tab_split_export(ui),
                            }
                            assert!(bounds.expand(1.0).contains_rect(ui.min_rect()),
                                "{} extends outside its workspace: {:?} vs {bounds:?}",
                                tab as usize, ui.min_rect());
                        });
                });
            }
        }

        // Also verify the Denoiser & Original Cut Studio panel fits within the minimum workspace
        app.active_tab = ActiveTab::SplitAndExport;
        app.magnifier_scope_active = false;
        app.denoiser_panel_active = true;
        app.learn_noise_from_region(0.0, 2.0);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput { screen_rect: Some(workspace), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::none().inner_margin(16.0))
                    .show(ctx, |ui| {
                        let bounds = ui.max_rect();
                        app.render_tab_split_export(ui);
                        assert!(bounds.expand(1.0).contains_rect(ui.min_rect()),
                            "Denoiser panel extends outside its workspace: {:?} vs {bounds:?}",
                            ui.min_rect());
                    });
            });
        }

        // Check the dialog at the minimum window size, including the completion state.
        let screen = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 540.0));
        app.show_export = true;
        app.export_directory = PathBuf::from("/Music").join("Long folder name ".repeat(10));
        for outcome in [None, Some(ExportOutcome::Done { count: 1, folder: app.export_directory.clone() })] {
            app.export_outcome = outcome;
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ctx| {
                    app.render_export_dialog(ctx);
                });
            }
            let dialog = ctx.memory(|memory| memory.area_rect(egui::Id::new("export_dialog"))).unwrap();
            assert!(screen.contains_rect(dialog), "Export dialog exceeds the window: {dialog:?}");
        }
    }
}

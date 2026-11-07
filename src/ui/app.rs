use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use egui::{Color32, Pos2, RichText, Stroke, Ui, Vec2};
use crate::analysis::{analyze_samples, AudioAnalysisReport, LevelDiagnosis};
use crate::audio::device::{get_channel_pairs_for_device, list_input_devices, AudioDeviceInfo, ChannelPair};
use crate::audio::player::{AudioPlayer, DecodedAudio};
use crate::audio::recorder::{AudioRecorder, RecorderState, StereoWaveformPoint};
use crate::splitter::{export_tracks, MarkerSnapshot, NormalizationMode, TrackSplitter, TransientPoint};
use crate::ui::knob_widget::render_gain_knob_card;
use crate::ui::meter_widget::render_stereo_meter;
use crate::ui::theme::{
    draw_recessed_well, mode_button, panel_frame, panel_title, primary_button, record_button,
    studio_panel, studio_wordmark, time_display, ACCENT, BG_INSET, BG_PANEL, BG_WINDOW,
    BORDER_MID, LED_AMBER, LED_GREEN, LED_RED, TEXT_MUTED, TEXT_SECONDARY,
};
use crate::i18n::Language;
use crate::project::{sanitize_filename, TapeDigizerProject, TapeSide};
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
    region: (f64, f64),
    sensitivity: f32,
    points: std::sync::Arc<[TransientPoint]>,
}

pub struct TapeDigizerApp {
    // Internationalization
    pub language: Language,

    // Tape & Album Organization
    pub tape_name: String,
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

    // Timer Settings
    selected_timer_minutes: u32,
    enable_silence_autostop: bool,

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
    transient_sensitivity: f32,
    show_transients: bool,
    scope_zoom: f32,
    scope_gain_db: f32,
    scope_auto_gain: bool,
    scope_scroll: f32,
    waveform_scroll: f32,
    active_edit_tool: EditTool,

    // Hover Audition (Stationary Mouse Preview)
    hover_preview_enabled: bool,
    hover_mouse_pos: Option<Pos2>,
    hover_stationary_since: Option<std::time::Instant>,
    hover_audition_time: Option<f64>,

    // Export Options
    export_directory: PathBuf,
    normalization_mode: NormalizationMode,
    export_outcome: Option<ExportOutcome>,

    // General status/error
    ui_error_message: Option<String>,
    job: Option<PendingJob>,
    saved_edits: Option<SavedEdits>,
    pause_preview: Option<MarkerSnapshot>,
    show_tracks: bool,
    show_inspector: bool,
    show_export: bool,
    #[cfg(target_os = "macos")]
    mac_menu: Option<super::macos_menu::MacMenu>,
}

impl TapeDigizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(crate::ui::theme::studio_visuals());
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 7.0);
            style.spacing.button_padding = Vec2::new(9.0, 4.0);
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

        let timer_minutes = cfg.timer_minutes.unwrap_or(45);
        let silence_stop = cfg.silence_autostop.unwrap_or(true);
        let norm_mode = match cfg.normalization_mode.as_deref() {
            Some("track") => NormalizationMode::PeakPerTrack,
            Some("album") => NormalizationMode::PeakAlbum,
            _ => NormalizationMode::None,
        };

        let lang = cfg.language.unwrap_or(Language::De);
        let tape_name = cfg.last_tape_name.clone().unwrap_or_else(|| "Tonband 01".to_string());

        let app = Self {
            language: lang,
            tape_name,
            tape_side: TapeSide::SideA,
            current_project_path: None,
            current_audio_path: None,
            pending_delete_marker: None,
            active_tab: ActiveTab::Recording,
            available_devices: devices,
            selected_device_idx: selected_dev_idx,
            channel_pairs,
            selected_pair_idx,
            recorder: AudioRecorder::new(),
            player: AudioPlayer::new(),
            splitter: TrackSplitter::new(),
            selected_timer_minutes: timer_minutes,
            enable_silence_autostop: silence_stop,
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
            transient_sensitivity: 1.0,
            show_transients: true,
            scope_zoom: 1.0,
            scope_gain_db: 0.0,
            scope_auto_gain: true,
            scope_scroll: 0.0,
            waveform_scroll: 0.0,
            active_edit_tool: EditTool::Pointer,
            hover_preview_enabled: false,
            hover_mouse_pos: None,
            hover_stationary_since: None,
            hover_audition_time: None,
            export_directory: default_export_dir,
            normalization_mode: norm_mode,
            export_outcome: None,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            show_tracks: true,
            show_inspector: true,
            show_export: false,
            #[cfg(target_os = "macos")]
            mac_menu: None,
        };

        app.save_config();
        app
    }

    fn preview_marker_drag(&mut self, index: usize, time: f64) {
        if self.marker_drag_cancelled {
            return;
        }
        self.marker_drag.get_or_insert_with(|| self.splitter.snapshot());
        self.selected_marker_idx = Some(index);
        self.selected_segment_idx = Some(index + 1);
        self.splitter.preview_marker_move(index, time, self.player.total_seconds(), self.player.sample_rate());
    }

    fn nudge_marker(&mut self, index: usize, delta: f64, duration: f64) {
        if let Some(&time) = self.splitter.markers.get(index) {
            self.splitter.move_marker(index, time + delta, duration, self.player.sample_rate());
        }
    }

    fn reset_editor_selection(&mut self) {
        self.clear_hover_audition();
        self.selected_marker_idx = None;
        self.selected_segment_idx = None;
        self.pending_delete_marker = None;
        self.marker_drag = None;
        self.selected_region = None;
        self.pause_preview = None;
        self.show_export = false;
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
            silence_autostop: Some(self.enable_silence_autostop),
            normalization_mode: Some(norm_str.to_string()),
            language: Some(self.language),
            last_tape_name: Some(self.tape_name.clone()),
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
                        name, TapeSide::SideA, String::new(), Vec::new(), Vec::new(), Vec::new(), "none".into(),
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
        self.start_job(ctx, self.text("Pausen werden gesucht …", "Finding pauses …"), move || {
            let mut proposal = TrackSplitter::new();
            if gaps {
                proposal.detect_silence_pauses_as_gaps(&left, &right, rate, -45.0, 1.8);
            } else {
                proposal.detect_silence_pauses(&left, &right, rate, -45.0, 1.8);
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
            ctx.request_repaint_after(Duration::from_millis(16));
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
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            if let Some(before) = self.marker_drag.take() {
                self.splitter.commit_edit(before);
            }
            self.marker_drag_cancelled = false;
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
                }
                if let Some(m_idx) = self.selected_marker_idx {
                    let total_sec = self.player.total_seconds();
                    let delta = if shift { 1.0 } else { 0.1 };
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
                        ui.label(format!("● {label} {seconds} s"));
                    } else if is_recording {
                        let time = self.recorder.elapsed_duration().as_secs_f64();
                        ui.colored_label(LED_RED,
                            format!("● REC {}", timecode(time)));
                    } else if finalizing {
                        ui.spinner();
                        ui.label(self.text("WAV wird abgeschlossen …", "Finalizing WAV …"));
                    } else if is_monitoring {
                        ui.label(self.text("Pegelcheck aktiv", "Monitoring"));
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
                    });
                });
                if self.active_tab == ActiveTab::Recording {
                    ui.add_space(5.0);
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let label = match recorder_state {
                            RecorderState::WaitingForSignal => self.text("Warten abbrechen", "Cancel waiting"),
                            RecorderState::Recording => self.text("Stopp mit 1 Min. Nachlauf", "Stop with 1 min tail"),
                            RecorderState::RunOn => self.text("Nachlauf läuft", "Recording tail"),
                            _ => self.text("Aufnahme starten", "Start recording"),
                        };
                        let record = record_button(label);
                        let ready = waiting_for_signal || recorder_state == RecorderState::Recording
                            || (!self.recorder.is_busy() && self.job.is_none() && self.current_channel_pair().is_some());
                        if ui.add_enabled(ready, record).clicked() {
                            self.record_or_stop();
                        }
                        if let Some(device) = self.current_device() {
                            ui.label(&device.name);
                        }
                        if let Some(pair) = self.current_channel_pair() {
                            ui.label(pair.label);
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
        let timer = (self.selected_timer_minutes != 0)
            .then(|| Duration::from_secs(u64::from(self.selected_timer_minutes) * 60));
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
            if ui.add_enabled(total > 0.0, egui::Button::new(self.text("Marker setzen · M", "Add marker · M"))).clicked()
            {
                self.add_marker(position);
            }
            let tracks = self.text("Trackliste", "Track list");
            ui.toggle_value(&mut self.show_tracks, tracks);
            ui.toggle_value(&mut self.show_inspector, "Inspector");
            if ui.add_enabled(total > 0.0, primary_button(self.text("Exportieren … · ⇧⌘E", "Export … · ⇧⌘E"))).clicked() {
                self.export_page = 0;
                self.show_export = true;
            }
        });
    }

    fn add_marker(&mut self, position: f64) {
        if let Some(index) = self.splitter.add_marker(position, self.player.total_seconds(), self.player.sample_rate()) {
            self.select_marker(index);
            self.show_inspector = true;
        }
    }

    fn select_marker(&mut self, index: usize) {
        self.selected_marker_idx = Some(index);
        self.selected_segment_idx = Some(index + 1);
    }

    fn set_scope_region(&mut self, region: (f64, f64)) {
        self.selected_region = Some(region);
        self.scope_zoom = 1.0;
        self.scope_scroll = 0.0;
    }

    fn render_marker_inspector(&mut self, ui: &mut Ui, total: f64) {
        let Some(index) = self.selected_marker_idx else { return; };
        let Some(&time) = self.splitter.markers.get(index) else { return; };
        let rate = self.player.sample_rate();
        studio_panel(ui, self.text("Schnittmarke", "Cut marker"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("T{:02}", index + 2)).strong().monospace());
                let mut seconds = time;
                let edit = egui::DragValue::new(&mut seconds)
                    .range(0.0..=total)
                    .speed(0.001)
                    .custom_formatter(|seconds, _| timecode(seconds))
                    .custom_parser(parse_timecode)
                    .update_while_editing(false);
                if ui.add(edit).on_hover_text(self.text("Zeitcode bearbeiten", "Edit timecode")).changed() {
                    if ui.input(|input| input.pointer.primary_down() && input.pointer.is_decidedly_dragging()) {
                        self.preview_marker_drag(index, seconds);
                    } else {
                        self.splitter.move_marker(index, seconds, total, rate);
                    }
                }
                let frame = (time * f64::from(rate)).round() as i64;
                let mut requested = frame;
                let position = ui.add(
                    egui::DragValue::new(&mut requested).range(1..=self.player.total_samples().saturating_sub(1) as i64)
                        .speed(1.0).prefix("Frame ").update_while_editing(false),
                );
                if position.changed() {
                    let time = requested as f64 / f64::from(rate);
                    if position.dragged() {
                        self.preview_marker_drag(index, time);
                    } else {
                        self.splitter.move_marker(index, time, total, rate);
                    }
                }
                for (label, delta) in [
                    ("−10 ms", -0.01), ("−1", -1.0 / f64::from(rate)),
                    ("+1", 1.0 / f64::from(rate)), ("+10 ms", 0.01),
                ] {
                    if ui.small_button(label).on_hover_text(self.text("Schnittgrenze verschieben", "Move cut boundary")).clicked() {
                        self.nudge_marker(index, delta, total);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(self.text("Davor hören", "Hear before")).clicked() {
                    self.player.play_range((time - 2.0).max(0.0), time);
                }
                if ui.button(self.text("Danach hören", "Hear after")).clicked() {
                    self.player.play_range(time, (time + 2.0).min(total));
                }
                if ui.button(self.text("Detailzoom", "Detail zoom")).clicked() {
                    self.set_scope_region(((time - 1.5).max(0.0), (time + 1.5).min(total)));
                    self.magnifier_scope_active = true;
                }
                if ui.button(format!("{} · ⌫", self.language.btn_delete())).clicked() {
                    self.request_marker_delete(index);
                }
            });
        });
        ui.add_space(8.0);
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
                studio_panel(ui, self.text("Band & Ablage", "Tape & storage"), |ui| {
                    ui.label(RichText::new(self.text("Bandname", "Tape name")).color(TEXT_SECONDARY));
                    let width = ui.available_width();
                    let name = ui.add(egui::TextEdit::singleline(&mut self.tape_name)
                        .desired_width(width).min_size(Vec2::new(width, 30.0))
                        .hint_text(self.language.hint_tape_name()));
                    if name.lost_focus() {
                        self.save_config();
                    }
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
                    });
                    let folder = self.session_project_dir().display().to_string();
                    ui.add(egui::Label::new(RichText::new(&folder).color(TEXT_SECONDARY)).truncate())
                        .on_hover_text(folder);
                });
                ui.add_space(8.0);
                studio_panel(ui, self.text("Audioeingang", "Audio input"), |ui| {
                    let device = self.current_device().map(|device| format!("{}  ·  {} ch", device.name, device.channels))
                        .unwrap_or_else(|| self.language.label_no_device().into());
                    let selected = self.selected_device_idx;
                    egui::ComboBox::from_id_salt("device_combo")
                        .width(ui.available_width()).truncate().selected_text(device)
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
                    if ui.small_button(self.language.btn_rescan()).clicked() {
                        self.refresh_device_list();
                    }
                });
                ui.add_space(8.0);
                studio_panel(ui, self.text("Aufnahmedauer", "Recording duration"), |ui| {
                    let before = self.selected_timer_minutes;
                    ui.horizontal_wrapped(|ui| {
                        for (minutes, label) in [
                            (15, "15 min"), (30, "30 min"), (45, "45 min"), (60, "60 min"),
                            (0, self.language.preset_unlimited()),
                        ] {
                            if ui.add(mode_button(label, self.selected_timer_minutes == minutes)).clicked() {
                                self.selected_timer_minutes = minutes;
                            }
                        }
                    });
                    let auto_stop = self.text("Bei Bandende automatisch stoppen", "Stop automatically at tape end");
                    let changed = ui.checkbox(&mut self.enable_silence_autostop, auto_stop)
                        .on_hover_text(self.language.checkbox_silence_autostop()).changed();
                    if changed || self.selected_timer_minutes != before {
                        self.save_config();
                    }
                });
            });

            let signal = &mut columns[1];
            studio_panel(signal, self.text("Eingangspegel", "Input levels"), |ui| {
                let monitoring = self.recorder.state() == RecorderState::Monitoring;
                let label = if monitoring {
                    self.text("Pegelcheck beenden", "End level check")
                } else {
                    self.text("Pegel prüfen", "Check levels")
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
                render_stereo_meter(ui, &self.recorder.get_meter());
                ui.label(RichText::new(self.language.meter_instructions()).color(TEXT_SECONDARY).small());
            });
            signal.add_space(8.0);
            studio_panel(signal, self.text("Live-Wellenform", "Live waveform"), |ui| {
                let state = self.recorder.state();
                let recording = matches!(state, RecorderState::Recording | RecorderState::RunOn);
                let elapsed = self.recorder.elapsed_duration().as_secs_f64();
                let limit = (self.selected_timer_minutes > 0)
                    .then(|| f64::from(self.selected_timer_minutes) * 60.0 + 60.0);
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
                    } else {
                        ui.label(RichText::new("PCM 24-bit  ·  Stereo").color(TEXT_SECONDARY));
                    }
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
                ui.label(if self.language == Language::De { "Starte zuerst eine Aufnahme in Tab 1." } else { "Start a recording in Tab 1 first." });
            }
            ui.add_space(12.0);
            if ui.button(if self.language == Language::De { "Zurück zu Tab 1 (Aufnahme)" } else { "Back to Tab 1 (Recording)" }).clicked() {
                self.active_tab = ActiveTab::Recording;
            }
            return;
        };

        // Simulated Analog Zoom L-12 Gain Knob Card
        let (adjustment_db, is_clip) = match report.diagnosis {
            LevelDiagnosis::Clipped { recommended_cut_db, .. } => (-recommended_cut_db, true),
            LevelDiagnosis::TooQuiet { recommended_boost_db, .. } => (recommended_boost_db, false),
            LevelDiagnosis::Optimal { .. } => (0.0, false),
        };

        render_gain_knob_card(
            ui,
            self.language.card_knob_title(),
            report.max_peak_db,
            adjustment_db,
            is_clip,
        );

        ui.add_space(14.0);

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

        ui.add_space(18.0);

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
        ui.add_space(2.0);

        let total_sec = self.player.total_seconds();
        let cur_sec = self.player.current_seconds();
        let has_audio = total_sec > 0.1 && !self.waveform_cache.levels.is_empty();

        if !has_audio {
            ui.add_space(4.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(34, 36, 44))
                .stroke(Stroke::new(1.0, BORDER_MID))
                .rounding(5.0)
                .inner_margin(Vec2::new(14.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ℹ").monospace().strong().color(LED_AMBER));
                        let note = if self.language == Language::De {
                            "Noch keine Audiodatei geladen. Starte eine Aufnahme in Tab 1 oder öffne ein gespeichertes Projekt (⌘O)."
                        } else {
                            "No audio recorded or loaded yet. Start recording in Tab 1 or open an existing project (⌘O)."
                        };
                        ui.label(RichText::new(note).color(TEXT_SECONDARY));
                    });
                });
        }

        ui.add_space(8.0);

        let show_scope = self.magnifier_scope_active && has_audio;
        if show_scope {
            ui.columns(2, |columns| {
                let timeline = &mut columns[0];
                self.render_timeline(timeline, total_sec, cur_sec, has_audio, 148.0);
                self.render_pause_proposal(timeline);
                let tracks = self.splitter.get_tracks(total_sec);
                self.render_track_list(timeline, &tracks);

                let details = &mut columns[1];
                self.render_detail_panel(details, total_sec, cur_sec);
                if self.show_inspector {
                    self.render_marker_inspector(details, total_sec);
                }
            });
        } else {
            let height = (ui.available_height() * 0.37).clamp(148.0, 300.0);
            self.render_timeline(ui, total_sec, cur_sec, has_audio, height);
            if self.show_inspector {
                self.render_marker_inspector(ui, total_sec);
            }
            self.render_pause_proposal(ui);
            let tracks = self.splitter.get_tracks(total_sec);
            self.render_track_list(ui, &tracks);
        }
    }

    fn render_timeline(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64, has_audio: bool, waveform_height: f32) {
        // Stereo waveform
        studio_panel(ui, self.text("Stereo-Wellenform", "Stereo waveform"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (tool, name, key) in [
                    (EditTool::Pointer, self.text("↖ Zeiger", "↖ Pointer"), "V"),
                    (EditTool::Scissor, self.text("│ Schneiden", "│ Split"), "S"),
                    (EditTool::Select, self.text("Bereich", "Range"), "R"),
                ] {
                    if ui.add(mode_button(format!("{name} · {key}"), self.active_edit_tool == tool)).clicked() {
                        self.active_edit_tool = tool;
                    }
                }
            });
            ui.horizontal(|ui| {
                let preview = self.text("Hover-Vorhören", "Hover audition");
                let detail = self.text("Detailansicht", "Detail view");
                ui.toggle_value(&mut self.hover_preview_enabled, format!("{preview} · H"));
                ui.toggle_value(&mut self.magnifier_scope_active, format!("{detail} · L"));
                ui.separator();
                if ui.add_enabled(self.splitter.can_undo(), egui::Button::new("↶ ⌘Z"))
                    .on_hover_text(self.language.btn_undo()).clicked()
                {
                    self.undo_marker_edit(false);
                }
                if ui.add_enabled(self.splitter.can_redo(), egui::Button::new("↷ ⇧⌘Z"))
                    .on_hover_text(self.language.btn_redo()).clicked()
                {
                    self.undo_marker_edit(true);
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
                self.magnifier_scope_active = true;
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
            }

            self.update_hover_audition(ui, &interaction, has_audio);

            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                ui.menu_button(self.text("Pausen erkennen", "Detect pauses"), |ui| {
                    if ui.add_enabled(has_audio, egui::Button::new(self.language.btn_auto_detect_pauses())).clicked() {
                        self.preview_pauses(ui.ctx(), false);
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_audio, egui::Button::new(self.language.btn_auto_detect_pauses_gaps())).clicked() {
                        self.preview_pauses(ui.ctx(), true);
                        ui.close_menu();
                    }
                });
                if ui.add_enabled(has_audio && !self.splitter.markers.is_empty(),
                    egui::Button::new(self.language.btn_clear_markers())).clicked()
                {
                    self.splitter.clear_markers();
                    self.selected_marker_idx = None;
                    self.selected_segment_idx = None;
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = 64.0;
                ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0)
                    .text("Zoom").suffix("×"));
                let gain = self.text("Pegel", "Gain");
                ui.add(egui::Slider::new(&mut self.waveform_vertical_zoom, 1.0..=16.0)
                    .text(gain).suffix("×"));
                if ui.button(self.text("Ganzes Band", "Fit tape")).clicked() {
                    self.waveform_zoom = 1.0;
                    self.waveform_vertical_zoom = 1.0;
                    self.waveform_scroll = 0.0;
                }
            });
        });
    }

    fn render_pause_proposal(&mut self, ui: &mut Ui) {
        if let Some((cuts, gaps)) = self.pause_preview.as_ref().map(|p| (p.markers.len(), p.pause_indices.len())) {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{}: {} · {}: {}", self.text("Vorgeschlagene Schnitte", "Suggested cuts"),
                    cuts, self.text("Pausen", "Gaps"), gaps));
                    if ui.add(primary_button(self.text("Übernehmen", "Apply")))
                        .on_hover_text(self.text("Ersetzt die Trackliste. Mit ⌘Z rückgängig machen.", "Replaces the track list. Undo with ⌘Z.")).clicked() {
                        let before = self.splitter.snapshot();
                        self.splitter.restore(self.pause_preview.take().unwrap());
                        self.splitter.commit_edit(before);
                        self.reset_editor_selection();
                    }
                    if ui.button(self.text("Verwerfen", "Discard")).clicked() {
                        self.pause_preview = None;
                    }
            });
        }

    }

    fn render_detail_panel(&mut self, ui: &mut Ui, total_sec: f64, cur_sec: f64) {
        studio_panel(ui, self.text("Detailansicht", "Detail view"), |ui| {
            let active_region = self.selected_region.unwrap_or_else(|| {
                if let Some(m_idx) = self.selected_marker_idx {
                    let m_time = self.splitter.markers.get(m_idx).copied().unwrap_or(cur_sec).floor();
                    ((m_time - 1.5).max(0.0), (m_time + 1.5).min(total_sec))
                } else if let Some(s_idx) = self.selected_segment_idx {
                    let tracks = self.splitter.get_tracks(total_sec);
                    if let Some(t) = tracks.get(s_idx) {
                        (t.start_seconds, t.end_seconds)
                    } else {
                        ((cur_sec.floor() - 1.5).max(0.0), (cur_sec.floor() + 1.5).min(total_sec))
                    }
                } else {
                    ((cur_sec.floor() - 1.5).max(0.0), (cur_sec.floor() + 1.5).min(total_sec))
                }
            });

            if self.selected_region.is_none() {
                self.set_scope_region(active_region);
            }

            let samples_l = self.player.get_samples_l();
            let samples_r = self.player.get_samples_r();
            let rate = self.player.sample_rate();

            let transient_region = if active_region.1 - active_region.0 > 8.0 {
                let center = self.selected_marker_idx.and_then(|i| self.splitter.markers.get(i).copied())
                    .unwrap_or(active_region.0 + 4.0);
                let start = (center - 4.0).clamp(active_region.0, active_region.1 - 8.0);
                // Keep the search window stable during small edits.
                let start = start.floor().max(active_region.0);
                (start, (start + 8.0).min(active_region.1))
            } else {
                active_region
            };
            let transients = if self.show_transients && self.marker_drag.is_none()
                && !ui.input(|input| input.pointer.primary_down()) {
                let sensitivity = self.transient_sensitivity;
                if !self.transient_cache.as_ref().is_some_and(|cache| {
                    cache.region == transient_region && cache.sensitivity == sensitivity
                }) {
                    self.transient_cache = Some(TransientCache {
                        region: transient_region,
                        sensitivity,
                        points: crate::splitter::detect_transients_in_range(
                            &samples_l, &samples_r, rate, transient_region.0, transient_region.1, sensitivity,
                        ).into(),
                    });
                }
                self.transient_cache.as_ref().unwrap().points.clone()
            } else {
                std::sync::Arc::from([])
            };

            ui.horizontal_wrapped(|ui| {
                time_display(ui, format!("{}  →  {}", timecode(active_region.0), timecode(active_region.1)));
                let label = self.text("Transienten", "Transients");
                ui.toggle_value(&mut self.show_transients, label);
            });

            let time_zoom = self.text("Zeitzoom", "Time zoom");
            let auto_gain = self.text("Auto-Pegel", "Auto gain");
            let vertical_gain = self.text("Pegel", "Gain");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().slider_width = 68.0;
                if ui.button(self.text("Einpassen", "Fit")).clicked() {
                    self.scope_zoom = 1.0;
                    self.scope_scroll = 0.0;
                }
                if ui.add(egui::Slider::new(&mut self.scope_zoom, 1.0..=50.0)
                    .logarithmic(true).text(time_zoom).suffix("×")).changed()
                {
                    self.scope_scroll = self.scope_scroll.min(1.0 - 1.0 / self.scope_zoom);
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = 68.0;
                ui.checkbox(&mut self.scope_auto_gain, auto_gain);
                ui.add(egui::Slider::new(&mut self.scope_gain_db, -24.0..=36.0)
                    .step_by(1.0).text(vertical_gain).suffix(" dB"));
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().slider_width = 68.0;
                if self.show_transients {
                    ui.add(egui::Slider::new(&mut self.transient_sensitivity, 0.4..=3.0)
                        .text(self.language.label_sensitivity()).suffix("×"));
                }
                if ui.button(self.text("Ganzer Track", "Full track")).clicked() {
                    let tracks = self.splitter.get_tracks(total_sec);
                    if let Some(track) = tracks.iter().find(|t| cur_sec >= t.start_seconds && cur_sec <= t.end_seconds) {
                        self.set_scope_region((track.start_seconds, track.end_seconds));
                    }
                }
                if ui.button(self.text("3 s Fokus", "3 s focus")).clicked() {
                    self.set_scope_region(((cur_sec - 1.5).max(0.0), (cur_sec + 1.5).min(total_sec)));
                }
            });

            let scope_action = crate::ui::waveform_widget::render_detail_magnifier_scope(
                ui,
                self.waveform_cache.for_view(total_sec / (active_region.1 - active_region.0) * f64::from(self.scope_zoom)),
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
                self.scope_zoom,
                &mut self.scope_scroll,
                cur_sec,
                self.player.is_playing(),
            );

            if let Some(t_seek) = scope_action.seek_to_time {
                self.player.seek_seconds(t_seek);
            }
            if let Some(t_trans) = scope_action.clicked_transient_time {
                self.add_marker(t_trans);
                self.player.seek_seconds(t_trans);
            }
            if let Some(s_idx) = scope_action.selected_marker_idx {
                self.select_marker(s_idx);
            }
            if let Some((idx, time)) = scope_action.dragged_marker {
                self.preview_marker_drag(idx, time);
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let play_label = if self.player.is_playing() {
                    self.text("Pause", "Pause")
                } else {
                    self.text("▶ Auswahl", "▶ Selection")
                };
                if ui.button(play_label).clicked() {
                    self.player.toggle_range(active_region.0, active_region.1);
                }

                if ui.button(self.text("│ Anfang", "│ Start")).clicked() {
                    self.add_marker(active_region.0);
                }

                if ui.button(self.text("│ Ende", "│ End")).clicked() {
                    self.add_marker(active_region.1);
                }

                if let Some((index, time)) = self.selected_marker_idx
                    .and_then(|i| self.splitter.markers.get(i).map(|&time| (i, time)))
                {
                    let closest = transients.iter().min_by(|a, b| {
                        (a.time_sec - time).abs().total_cmp(&(b.time_sec - time).abs())
                    }).filter(|point| (point.time_sec - time).abs() < 2.0);
                    let snap = self.text("Einrasten", "Snap");
                    if ui.add_enabled(closest.is_some(), egui::Button::new(snap)).clicked() {
                        self.splitter.move_marker(index, closest.unwrap().time_sec, total_sec, rate);
                    }
                }
            });
        });
    }

    fn render_track_list(&mut self, ui: &mut Ui, tracks: &[crate::splitter::TrackInfo]) {
        if !self.show_tracks {
            return;
        }
        if self.splitter.custom_titles.len() < tracks.len() {
            self.splitter.custom_titles.resize(tracks.len(), String::new());
        }

        let per_page = ((ui.available_height() - 66.0) / 108.0).floor().max(1.0) as usize;
        let pages = tracks.len().div_ceil(per_page).max(1);
        self.track_page = self.track_page.min(pages - 1);
        panel_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    panel_title(ui, self.text("Trackliste", "Track list"));
                    ui.label(RichText::new(tracks.len().to_string()).color(TEXT_SECONDARY));
                    page_controls(ui, &mut self.track_page, pages, self.language);
                });
                ui.add_space(6.0);
                let start = self.track_page * per_page;
                for (index, track) in tracks.iter().enumerate().skip(start).take(per_page) {
                    let selected = self.selected_segment_idx == Some(index);
                    let border = if selected { ACCENT } else { BORDER_MID };
                    egui::Frame::none()
                        .fill(BG_WINDOW)
                        .stroke(Stroke::new(1.0, border))
                        .rounding(8.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let badge = if track.is_pause { "Ⅱ".into() } else { format!("{:02}", track.index) };
                                let badge = egui::Button::new(RichText::new(badge).monospace()).selected(selected);
                                if ui.add_sized([38.0, 32.0], badge).clicked() {
                                    self.selected_segment_idx = Some(index);
                                    self.selected_marker_idx = index.checked_sub(1);
                                    self.player.seek_seconds(track.start_seconds);
                                }

                                if track.is_pause {
                                    ui.label(RichText::new(self.language.badge_pause()).color(LED_AMBER));
                                } else {
                                    let width = ui.available_width();
                                    let response = ui.add(
                                        egui::TextEdit::singleline(&mut self.splitter.custom_titles[index])
                                            .id(egui::Id::new("track_title_field").with(index))
                                            .desired_width(width)
                                            .min_size(Vec2::new(width, 32.0))
                                            .margin(egui::Margin::symmetric(10.0, 6.0))
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
                            });

                            ui.add_space(5.0);
                            ui.horizontal_wrapped(|ui| {
                                let start = RichText::new(timecode(track.start_seconds)).monospace();
                                if index > 0 {
                                    if ui.selectable_label(selected, start).clicked() {
                                        self.select_marker(index - 1);
                                        self.show_inspector = true;
                                    }
                                } else {
                                    ui.label(start);
                                }
                                ui.label(RichText::new(track.duration_formatted()).monospace().color(TEXT_SECONDARY));
                                ui.separator();
                                if ui.small_button(self.text("Anhören", "Audition")).clicked() {
                                    self.player.play_range(track.start_seconds, track.end_seconds);
                                }
                                let action = if track.is_pause { self.language.btn_make_track() } else { self.language.btn_make_pause() };
                                if ui.small_button(action).clicked() {
                                    self.splitter.toggle_pause(index);
                                }
                                if index > 0 && ui.small_button(self.text("× Marke", "× Marker"))
                                    .on_hover_text(self.text("Schnittmarke am Trackanfang entfernen", "Remove marker at track start"))
                                    .clicked()
                                {
                                    self.request_marker_delete(index - 1);
                                }
                            });
                        });
                    ui.add_space(5.0);
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
            .frame(egui::Frame::window(&ctx.style()).inner_margin(22.0).rounding(14.0))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.heading(self.text("Tracks exportieren", "Export tracks"));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{}  ·  {}", self.tape_name, self.tape_side.label(self.language)))
                    .color(TEXT_SECONDARY));
                ui.add_space(14.0);

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
                let mut files = Vec::new();
                for track in tracks.iter().filter(|track| !track.is_pause) {
                    let name = track.export_filename();
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

                ui.add_space(12.0);
                panel_title(ui, self.text("Audio", "Audio"));
                ui.label(RichText::new(format!("WAV  ·  PCM 24-bit  ·  Stereo  ·  {} Hz", self.player.sample_rate()))
                    .color(TEXT_SECONDARY));
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

                ui.add_space(12.0);
                let per_page = ((screen.height() - 414.0) / 30.0).floor().clamp(3.0, 10.0) as usize;
                let pages = count.div_ceil(per_page).max(1);
                self.export_page = self.export_page.min(pages - 1);
                ui.horizontal(|ui| {
                    panel_title(ui, &format!("{} {count}", self.text("Dateien", "Files")));
                    page_controls(ui, &mut self.export_page, pages, self.language);
                });
                let gaps = tracks.len() - count;
                if gaps > 0 {
                    ui.label(RichText::new(format!("{} {gaps}", self.text("Ausgelassene Pausen", "Excluded gaps")))
                        .color(TEXT_SECONDARY));
                }
                if count == 0 {
                    ui.label(self.text("Keine Tracks zum Exportieren.", "No tracks to export."));
                }
                for (track, name, exists) in files.iter().skip(self.export_page * per_page).take(per_page) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.monospace(track.duration_formatted());
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
                ui.add_space(12.0);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = self.text("Exportieren", "Export");
                    let export = primary_button(label);
                    if ui.add_enabled(export_enabled, export).clicked() && self.effective_export_directory() == target {
                        if let Some(source) = self.current_audio_path.clone() {
                            self.export_outcome = None;
                            let mode = self.normalization_mode.clone();
                            let progress = Arc::new(ExportProgress::default());
                            let worker_progress = progress.clone();
                            self.start_job(ctx, self.text("Tracks werden exportiert …", "Exporting tracks …"), move || {
                                let files = export_tracks(&source, &target, &tracks, mode, |done, total| {
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
            enable_silence_autostop: true,
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
            export_outcome: None,
            ui_error_message: None,
            job: None,
            saved_edits: None,
            pause_preview: None,
            show_tracks: true,
            show_inspector: true,
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

        // A one-track project with a short name must still get a wide input.
        app.splitter.custom_titles = vec!["cghcg".into()];
        let tracks = app.splitter.get_tracks(1800.0);
        for (window_width, minimum_field_width) in [(800.0, 650.0), (1140.0, 990.0)] {
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

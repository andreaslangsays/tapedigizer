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
    draw_recessed_well, BG_CARD, BG_INSET, BG_LCD, BG_PANEL, BG_WINDOW,
    BORDER_DARK, BORDER_LIGHT, BORDER_MID, LED_AMBER, LED_GREEN, LED_RED, TEXT_MUTED,
    TEXT_PRIMARY, TEXT_SECONDARY,
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
    waveform_height: f32,
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
    hover_is_auditioning: bool,
    hover_audition_time: Option<f64>,

    // Export Options
    export_directory: PathBuf,
    normalization_mode: NormalizationMode,
    export_status_message: Option<(bool, String)>,

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
        // Authentic Logic Pro / macOS dark chrome styling
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(TEXT_PRIMARY);
        visuals.panel_fill = BG_WINDOW;
        visuals.window_fill = BG_WINDOW;

        visuals.widgets.noninteractive.bg_fill = BG_PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_DARK);

        visuals.widgets.inactive.bg_fill = Color32::from_rgb(48, 48, 54);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_MID);

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(60, 60, 68);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_LIGHT);

        visuals.widgets.active.bg_fill = Color32::from_rgb(37, 99, 235);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(96, 165, 250));

        visuals.widgets.inactive.rounding = egui::Rounding::same(5.0);
        visuals.widgets.hovered.rounding = egui::Rounding::same(5.0);
        visuals.widgets.active.rounding = egui::Rounding::same(5.0);

        cc.egui_ctx.set_visuals(visuals);

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
            waveform_height: 280.0,
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
            hover_is_auditioning: false,
            hover_audition_time: None,
            export_directory: default_export_dir,
            normalization_mode: norm_mode,
            export_status_message: None,
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
        self.selected_marker_idx = None;
        self.selected_segment_idx = None;
        self.pending_delete_marker = None;
        self.marker_drag = None;
        self.selected_region = None;
        self.pause_preview = None;
        self.show_export = false;
        self.transient_cache = None;
        self.magnifier_scope_active = false;
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
                self.export_status_message = None;
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
                self.export_status_message = Some((true, format!(
                    "{} {count} · {}", self.text("Exportierte Tracks", "Exported tracks"), folder.display(),
                )));
            }
            Ok(JobResult::Pauses(snapshot)) => self.pause_preview = Some(snapshot),
            Ok(JobResult::Saved(path, audio_path, edits)) => {
                self.apply_saved_project(path, audio_path, edits);
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
        self.export_status_message = None;
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
        self.export_status_message = Some((
            true,
            if self.language == Language::De { "Neues Projekt initialisiert.".into() } else { "New project initialized.".into() }
        ));
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
        let is_recording = self.recorder.state() == RecorderState::Recording;
        let finalizing = self.recorder.state() == RecorderState::Finalizing;
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
        if self.active_tab == ActiveTab::SplitAndExport && !ctx.wants_keyboard_input() && self.job.is_none() && !is_recording && !finalizing {
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
                if !self.hover_preview_enabled && self.hover_is_auditioning {
                    self.player.pause();
                    self.hover_is_auditioning = false;
                    self.hover_audition_time = None;
                }
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
                self.active_edit_tool = if self.active_edit_tool == EditTool::Scissor {
                    EditTool::Pointer
                } else {
                    EditTool::Scissor
                };
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

        #[cfg(target_os = "macos")]
        let needs_menu = self.mac_menu.is_none();
        #[cfg(not(target_os = "macos"))]
        let needs_menu = true;
        if needs_menu {
        egui::TopBottomPanel::top("app_menu_bar")
            .frame(egui::Frame::none().fill(Color32::from_rgb(20, 22, 26)).inner_margin(egui::Margin { left: 12.0, right: 12.0, top: 4.0, bottom: 4.0 }))
            .show(ctx, |ui| {
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
                            self.export_status_message = Some((true, format!("TapeDigizer v{} — High-fidelity Audio Digitization Console", env!("CARGO_PKG_VERSION"))));
                            ui.close_menu();
                        }
                    });
                });
            });

        }

        let light_chrome = ctx.system_theme() == Some(egui::Theme::Light);
        let toolbar_fill = if light_chrome { Color32::from_gray(235) } else { BG_PANEL };
        egui::TopBottomPanel::top("workspace_toolbar")
            .frame(egui::Frame::none().fill(toolbar_fill).inner_margin(12.0))
            .show(ctx, |ui| {
                if light_chrome {
                    ui.style_mut().visuals = egui::Visuals::light();
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("TapeDigizer").size(16.0).strong());
                    ui.separator();
                    for (tab, label) in [
                        (ActiveTab::Recording, self.text("Aufnahme", "Record")),
                        (ActiveTab::Diagnosis, self.text("Pegel", "Levels")),
                        (ActiveTab::SplitAndExport, self.text("Schnitt", "Edit")),
                    ] {
                        if ui.add_enabled(!self.recorder.is_busy() || tab == ActiveTab::Recording, egui::SelectableLabel::new(self.active_tab == tab, label)).clicked() {
                            self.active_tab = tab;
                        }
                    }
                    ui.separator();
                    if is_recording {
                        let time = self.recorder.elapsed_duration().as_secs_f64();
                        ui.colored_label(if light_chrome { Color32::DARK_RED } else { LED_RED },
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
                        let (label, fill) = if is_recording {
                            (self.text("Aufnahme stoppen", "Stop recording"), Color32::from_rgb(176, 51, 49))
                        } else {
                            (self.text("Aufnahme starten", "Start recording"), Color32::from_rgb(176, 51, 49))
                        };
                        let record = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
                            .fill(fill).rounding(6.0);
                        let ready = is_recording || (!finalizing && self.job.is_none() && self.current_channel_pair().is_some());
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
                    ui.add_enabled_ui(self.job.is_none() && !is_recording && !finalizing, |ui| self.render_transport(ui));
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

                if let Some(job) = &self.job {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(job.label);
                        if let Some(progress) = &job.export_progress {
                            let total = progress.total.load(Ordering::Relaxed);
                            let finished = progress.finished.load(Ordering::Relaxed);
                            if total != 0 {
                                ui.add(egui::ProgressBar::new(finished as f32 / total as f32)
                                    .desired_width(120.0).text(format!("{finished}/{total}")));
                            }
                            if ui.add_enabled(!progress.cancelled.load(Ordering::Relaxed),
                                egui::Button::new(self.language.btn_cancel())).clicked() {
                                progress.cancelled.store(true, Ordering::Relaxed);
                            }
                            ctx.request_repaint_after(Duration::from_millis(150));
                        }
                    });
                }
                let enabled = self.job.is_none() && !finalizing && (!is_recording || self.active_tab == ActiveTab::Recording);
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ScrollArea::vertical().id_salt("workspace").show(ui, |ui| {
                        match self.active_tab {
                            ActiveTab::Recording => self.render_tab_recording(ui),
                            ActiveTab::Diagnosis => self.render_tab_diagnosis(ui),
                            ActiveTab::SplitAndExport => self.render_tab_split_export(ui),
                        }
                    });
                });
            });
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
        if self.recorder.state() == RecorderState::Recording {
            self.recorder.begin_stop();
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
            ui.label(RichText::new(format!("{} / {}", timecode(position), timecode(total))).monospace());
            ui.separator();
            if ui.add_enabled(total > 0.0, egui::Button::new(self.text("Marker setzen", "Add marker")))
                .on_hover_text("M").clicked()
            {
                self.add_marker(position);
            }
            let tracks = self.text("Trackliste", "Track list");
            ui.toggle_value(&mut self.show_tracks, tracks);
            ui.toggle_value(&mut self.show_inspector, "Inspector");
            if ui.add_enabled(total > 0.0, egui::Button::new(self.text("Exportieren …", "Export …"))).clicked() {
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
        render_hardware_rack_card(ui, self.text("Schnittmarke", "Cut marker"), |ui| {
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
                if ui.button(self.language.btn_delete()).clicked() {
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
        ui.add_space(2.0);
        let is_recording = self.recorder.state() == RecorderState::Recording;
        ui.add_enabled_ui(!is_recording, |ui| {

        // Rack Panel 0: Cassette Metadata & Project Location
        render_hardware_rack_card(ui, self.text("Band und Ablage", "Tape and storage"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.language.label_tape_name()).color(TEXT_SECONDARY).strong());
                let name_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.tape_name)
                        .desired_width(220.0)
                        .hint_text(self.language.hint_tape_name())
                );
                if name_edit.lost_focus() {
                    self.save_config();
                }

                ui.add_space(14.0);
                ui.label(RichText::new(self.language.label_tape_side()).color(TEXT_SECONDARY).strong());

                let is_side_a = self.tape_side == TapeSide::SideA;
                let btn_a = egui::Button::new(
                    RichText::new(format!("● {}", TapeSide::SideA.label(self.language)))
                        .strong()
                        .color(if is_side_a { Color32::WHITE } else { TEXT_MUTED })
                )
                .fill(if is_side_a { Color32::from_rgb(37, 99, 235) } else { Color32::from_rgb(38, 38, 44) })
                .stroke(Stroke::new(1.0, if is_side_a { Color32::from_rgb(96, 165, 250) } else { BORDER_MID }))
                .rounding(4.0);

                if ui.add(btn_a).clicked() {
                    self.tape_side = TapeSide::SideA;
                }

                let is_side_b = self.tape_side == TapeSide::SideB;
                let btn_b = egui::Button::new(
                    RichText::new(format!("● {}", TapeSide::SideB.label(self.language)))
                        .strong()
                        .color(if is_side_b { Color32::BLACK } else { TEXT_MUTED })
                )
                .fill(if is_side_b { LED_AMBER } else { Color32::from_rgb(38, 38, 44) })
                .stroke(Stroke::new(1.0, if is_side_b { Color32::WHITE } else { BORDER_MID }))
                .rounding(4.0);

                if ui.add(btn_b).clicked() {
                    self.tape_side = TapeSide::SideB;
                }
            });

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.language.label_project_folder()).color(TEXT_MUTED));
                let folder_display = self.session_project_dir();
                ui.label(RichText::new(format!("📁 {}", folder_display.to_string_lossy())).color(Color32::from_rgb(56, 189, 248)).monospace().small());

                if ui.button(self.language.btn_change_folder()).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.export_directory = folder;
                        self.save_config();
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("🛡️ {}", self.language.hint_auto_saved())).color(LED_GREEN).small().italics());
                });
            });
        });

        ui.add_space(12.0);

        // Rack Panel 1: Hardware Input Routing
        render_hardware_rack_card(ui, self.text("Audioeingang", "Audio input"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.language.label_interface()).color(TEXT_SECONDARY));
                egui::ComboBox::from_id_salt("device_combo")
                    .width(300.0)
                    .selected_text(
                        self.current_device()
                            .map(|d| format!("{} ({} Ch)", d.name, d.channels))
                            .unwrap_or_else(|| self.language.label_no_device().into()),
                    )
                    .show_ui(ui, |ui| {
                        for (idx, dev) in self.available_devices.iter().enumerate() {
                            let resp = ui.selectable_value(&mut self.selected_device_idx, idx, &dev.name);
                            if resp.changed() || resp.clicked() {
                                self.channel_pairs = get_channel_pairs_for_device(dev);
                                self.selected_pair_idx = 0;
                                self.save_config();
                            }
                        }
                    });

                ui.add_space(16.0);

                ui.label(RichText::new(self.language.label_stereo_input()).color(TEXT_SECONDARY));
                egui::ComboBox::from_id_salt("channel_combo")
                    .width(320.0)
                    .selected_text(
                        self.current_channel_pair()
                            .map(|p| p.label)
                            .unwrap_or_else(|| self.language.label_select_channel().into()),
                    )
                    .show_ui(ui, |ui| {
                        for (idx, pair) in self.channel_pairs.iter().enumerate() {
                            let resp = ui.selectable_value(&mut self.selected_pair_idx, idx, &pair.label);
                            if resp.changed() || resp.clicked() {
                                self.save_config();
                            }
                        }
                    });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(self.language.btn_rescan()).clicked() {
                        self.refresh_device_list();
                    }
                });
            });
        });

        ui.add_space(12.0);

        });

        // Rack Panel 2: Studio LED Ladder Peak/VU Meters
        render_hardware_rack_card(ui, self.text("Eingangspegel", "Input levels"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.language.meter_instructions()).color(TEXT_SECONDARY).italics());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_monitoring = self.recorder.state() == RecorderState::Monitoring;
                    let (btn_text, btn_bg, btn_stroke) = if is_monitoring {
                        (self.language.btn_stop_level_check(), Color32::from_rgb(180, 40, 40), Color32::from_rgb(231, 76, 60))
                    } else {
                        (self.language.btn_start_level_check(), Color32::from_rgb(37, 99, 235), Color32::from_rgb(96, 165, 250))
                    };

                    let mon_btn = egui::Button::new(RichText::new(btn_text).color(Color32::WHITE).strong())
                        .fill(btn_bg)
                        .stroke(Stroke::new(1.0, btn_stroke))
                        .rounding(5.0)
                        .min_size(Vec2::new(180.0, 30.0));

                    if ui.add_enabled(!is_recording && (is_monitoring || self.current_channel_pair().is_some()), mon_btn).clicked() {
                        if is_monitoring {
                            self.recorder.stop_monitoring();
                        } else {
                            let dev_name = self.current_device().map(|d| d.name.clone());
                            let pair = self.current_channel_pair();
                            if let (Some(name), Some(pair)) = (dev_name, pair) {
                                if let Err(e) = self.recorder.start_monitoring(&name, pair) {
                                    self.ui_error_message = Some(e);
                                }
                            }
                        }
                    }
                });
            });

            ui.add_space(8.0);
            let meter = self.recorder.get_meter();
            render_stereo_meter(ui, &meter);
        });

        ui.add_space(12.0);

        // Rack Panel 3: Recording Timer & Tactile Record Button
        render_hardware_rack_card(ui, self.text("Aufnahmedauer und Auto-Stopp", "Duration and auto-stop"), |ui| {
            ui.add_enabled_ui(!is_recording, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.language.label_duration_preset()).color(TEXT_SECONDARY));
                if ui.selectable_value(&mut self.selected_timer_minutes, 15, "15 Min").clicked()
                    || ui.selectable_value(&mut self.selected_timer_minutes, 30, "30 Min").clicked()
                    || ui.selectable_value(&mut self.selected_timer_minutes, 45, "45 Min (C-90)").clicked()
                    || ui.selectable_value(&mut self.selected_timer_minutes, 60, "60 Min (C-120)").clicked()
                    || ui.selectable_value(&mut self.selected_timer_minutes, 0, self.language.preset_unlimited()).clicked()
                {
                    self.save_config();
                }
            });

            ui.add_space(6.0);
            if ui.checkbox(
                &mut self.enable_silence_autostop,
                self.language.checkbox_silence_autostop(),
            ).changed() {
                self.save_config();
            }

            ui.add_space(16.0);

            });

            ui.horizontal_wrapped(|ui| {
                // Digital Studio LCD Display
                if is_recording {
                    let elapsed = self.recorder.elapsed_duration();
                    let elap_sec = elapsed.as_secs();
                    let elap_str = format!("{:02}:{:02}:{:02}", elap_sec / 3600, (elap_sec % 3600) / 60, elap_sec % 60);

                    ui.add_space(20.0);
                    let lcd = egui::Frame::none()
                        .fill(BG_LCD)
                        .stroke(Stroke::new(1.0, BORDER_DARK))
                        .rounding(6.0)
                        .inner_margin(Vec2::new(16.0, 8.0));

                    lcd.show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("REC").color(LED_RED).monospace().size(16.0).strong());
                            ui.label(RichText::new(elap_str).color(Color32::WHITE).monospace().size(22.0).strong());

                            if let Some(rem) = self.recorder.remaining_duration() {
                                let rem_sec = rem.as_secs();
                                let rem_str = format!("{:02}:{:02}", (rem_sec % 3600) / 60, rem_sec % 60);
                                ui.add_space(8.0);
                                ui.label(RichText::new(format!("(Rest: {} Min)", rem_str)).color(TEXT_SECONDARY).monospace());
                            }
                        });
                    });
                }
            });
        });

        ui.add_space(12.0);

        // Rack Panel 4: Live Recording Waveform
        render_hardware_rack_card(ui, self.text("Live-Wellenform", "Live waveform"), |ui| {
            let is_recording = self.recorder.state() == RecorderState::Recording;
            let elapsed_sec = self.recorder.elapsed_duration().as_secs_f64();
            let timer_limit_sec = if self.selected_timer_minutes > 0 {
                Some(self.selected_timer_minutes as f64 * 60.0)
            } else {
                None
            };
            let idle_text = if self.recorder.state() == RecorderState::Monitoring {
                self.language.live_waveform_monitoring()
            } else {
                self.language.live_waveform_idle()
            };

            self.recorder.with_waveform_data(|wf| {
                crate::ui::waveform_widget::render_live_recording_waveform(
                    ui,
                    wf,
                    elapsed_sec,
                    timer_limit_sec,
                    is_recording,
                    idle_text,
                );
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
            let card_w = (ui.available_width() - 36.0) / 4.0;

            let l_label = if self.language == Language::De { "PEAK KANAL 1 (L)" } else { "PEAK CHANNEL 1 (L)" };
            render_hardware_metric(ui, card_w, l_label, &format!("{:.1} dBFS", report.peak_left_db), Color32::from_rgb(56, 189, 248));
            ui.add_space(8.0);

            let r_label = if self.language == Language::De { "PEAK KANAL 2 (R)" } else { "PEAK CHANNEL 2 (R)" };
            render_hardware_metric(ui, card_w, r_label, &format!("{:.1} dBFS", report.peak_right_db), Color32::from_rgb(147, 130, 248));
            ui.add_space(8.0);

            let headroom = (-1.5 - report.max_peak_db).max(0.0);
            let head_label = if self.language == Language::De { "HEADROOM (-1.5 dB Ziel)" } else { "HEADROOM (-1.5 dB Target)" };
            render_hardware_metric(ui, card_w, head_label, &format!("{:.1} dB", headroom), LED_AMBER);
            ui.add_space(8.0);

            let (clip_str, clip_col) = if report.clipped_samples > 0 {
                (format!("{} Samples", report.clipped_samples), LED_RED)
            } else {
                ((if self.language == Language::De { "0 (Sauber)" } else { "0 (Clean)" }).into(), LED_GREEN)
            };
            let clip_label = if self.language == Language::De { "CLIPPING (VERZERRUNG)" } else { "CLIPPING (DISTORTION)" };
            render_hardware_metric(ui, card_w, clip_label, &clip_str, clip_col);
        });

        ui.add_space(18.0);

        // Tactile Action Buttons
        ui.horizontal(|ui| {
            let retry_btn = egui::Button::new(RichText::new(self.language.btn_retry()).size(14.0))
                .fill(Color32::from_rgb(52, 54, 62))
                .stroke(Stroke::new(1.0, BORDER_MID))
                .rounding(5.0)
                .min_size(Vec2::new(260.0, 42.0));

            if ui.add(retry_btn).clicked() {
                self.active_tab = ActiveTab::Recording;
            }

            ui.add_space(16.0);

            let next_btn = egui::Button::new(
                RichText::new(self.language.btn_proceed_to_split())
                    .size(14.0)
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(Color32::from_rgb(37, 99, 235))
            .stroke(Stroke::new(1.0, Color32::from_rgb(96, 165, 250)))
            .rounding(5.0)
            .min_size(Vec2::new(340.0, 42.0));

            if ui.add(next_btn).clicked() {
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

        // 2. Stereo Waveform Rack Card
        render_hardware_rack_card(ui, self.text("Stereo-Wellenform", "Stereo waveform"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (tool, name, key) in [
                    (EditTool::Pointer, self.text("Zeiger", "Pointer"), "V"),
                    (EditTool::Scissor, self.text("Schneiden", "Split"), "S"),
                    (EditTool::Select, self.text("Bereich", "Range"), "R"),
                ] {
                    if ui.selectable_label(self.active_edit_tool == tool, name).on_hover_text(key).clicked() {
                        self.active_edit_tool = tool;
                    }
                }
                ui.separator();
                let preview = self.text("Hover-Vorhören", "Hover audition");
                let detail = self.text("Detailansicht", "Detail view");
                ui.toggle_value(&mut self.hover_preview_enabled, preview).on_hover_text("H");
                ui.toggle_value(&mut self.magnifier_scope_active, detail).on_hover_text("L");
                ui.separator();
                if ui.add_enabled(self.splitter.can_undo(), egui::Button::new("↶"))
                    .on_hover_text(self.language.btn_undo()).clicked()
                {
                    self.undo_marker_edit(false);
                }
                if ui.add_enabled(self.splitter.can_redo(), egui::Button::new("↷"))
                    .on_hover_text(self.language.btn_redo()).clicked()
                {
                    self.undo_marker_edit(true);
                }
            });

            ui.add_space(2.0);
            ui.label(
                RichText::new(self.language.tip_navigation())
                    .size(11.0)
                    .color(TEXT_MUTED),
            );

            ui.add_space(6.0);

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
                self.waveform_height,
                &mut self.waveform_scroll,
                self.active_edit_tool,
                self.hover_is_auditioning,
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

            // Hover Audition Engine
            if self.hover_preview_enabled && has_audio && !ui.input(|i| i.pointer.primary_down())
                && (!self.player.is_playing() || self.hover_is_auditioning) {
                if interaction.is_mouse_over_waveform {
                    if let Some(mouse_pos) = interaction.mouse_pos_in_waveform {
                        let moved = self.hover_mouse_pos
                            .map(|p| p.distance(mouse_pos) > 6.0)
                            .unwrap_or(true);

                        if moved {
                            self.hover_mouse_pos = Some(mouse_pos);
                            self.hover_stationary_since = Some(std::time::Instant::now());
                            if self.hover_is_auditioning {
                                self.player.pause();
                                self.hover_is_auditioning = false;
                                self.hover_audition_time = None;
                            }
                        } else if let Some(since) = self.hover_stationary_since {
                            if !self.hover_is_auditioning && since.elapsed() >= std::time::Duration::from_millis(300) {
                                if let Some(target_t) = interaction.hovered_time_sec {
                                    self.player.seek_seconds(target_t);
                                    self.player.play();
                                    self.hover_is_auditioning = true;
                                    self.hover_audition_time = Some(target_t);
                                }
                            }
                        }
                        if !self.hover_is_auditioning {
                            ui.ctx().request_repaint_after(Duration::from_millis(300));
                        }
                    }
                } else {
                    if self.hover_is_auditioning {
                        self.player.pause();
                        self.hover_is_auditioning = false;
                        self.hover_audition_time = None;
                    }
                    self.hover_stationary_since = None;
                    self.hover_mouse_pos = None;
                }
            } else if self.hover_is_auditioning {
                self.player.pause();
                self.hover_is_auditioning = false;
                self.hover_audition_time = None;
            }

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
                ui.separator();
                ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0)
                    .text("Zoom").suffix("×"));
                ui.add(egui::Slider::new(&mut self.waveform_vertical_zoom, 1.0..=16.0)
                    .text(self.language.label_vertical_zoom()).suffix("×"));
                if ui.button(self.text("Ganzes Band", "Fit tape")).clicked() {
                    self.waveform_zoom = 1.0;
                    self.waveform_vertical_zoom = 1.0;
                    self.waveform_scroll = 0.0;
                }
            });
        });
        ui.add_space(8.0);
        if self.show_inspector {
            self.render_marker_inspector(ui, total_sec);
        }

        // 3. Detail-Lupenansicht (Magnifier Scope & Transient Analysis) when toggled
        if self.magnifier_scope_active && has_audio {
            ui.add_space(8.0);
            render_hardware_rack_card(ui, self.language.card_magnifier_scope(), |ui| {
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

                // Scope Header Bar with Region Info, Transient Toggle, Zoom & Sensitivity Slider
                ui.horizontal(|ui| {
                    let r_start = active_region.0;
                    let r_end = active_region.1;
                    let r_dur = (r_end - r_start).max(0.0);
                    let s_m = (r_start / 60.0).floor() as u64;
                    let s_s = r_start % 60.0;
                    let e_m = (r_end / 60.0).floor() as u64;
                    let e_s = r_end % 60.0;

                    let reg_badge = format!("🔍 BEREICH: {:02}:{:05.2} ➔ {:02}:{:05.2} ({:.2}s)", s_m, s_s, e_m, e_s, r_dur);
                    ui.label(RichText::new(reg_badge).monospace().strong().color(Color32::from_rgb(255, 215, 0)));

                    ui.add_space(6.0);

                    // Transients Ein-/Ausblenden Toggle
                    let trans_col = if self.show_transients { LED_GREEN } else { TEXT_MUTED };
                    let trans_label = if self.show_transients {
                        format!("⚡ Transienten: EIN ({})", transients.len())
                    } else {
                        "⚡ Transienten: AUS".to_string()
                    };
                    let trans_btn = egui::Button::new(RichText::new(trans_label).monospace().strong().color(trans_col))
                        .rounding(4.0);
                    if ui.add(trans_btn).on_hover_text("Transienten (Einsätze & Peaks) in der Lupe ein- oder ausblenden").clicked() {
                        self.show_transients = !self.show_transients;
                    }

                });

                let time_zoom = self.text("Zeitzoom", "Time zoom");
                let auto_gain = self.text("Automatische Pegelanhebung", "Auto gain");
                let vertical_gain = self.text("Vertikalzoom", "Vertical zoom");
                ui.horizontal_wrapped(|ui| {
                    if ui.button(self.text("Ganze Auswahl", "Fit selection")).clicked() {
                        self.scope_zoom = 1.0;
                        self.scope_scroll = 0.0;
                    }
                    if ui.add(egui::Slider::new(&mut self.scope_zoom, 1.0..=50.0)
                        .logarithmic(true).text(time_zoom).suffix("×")).changed()
                    {
                        self.scope_scroll = self.scope_scroll.min(1.0 - 1.0 / self.scope_zoom);
                    }
                    ui.checkbox(&mut self.scope_auto_gain, auto_gain);
                    ui.add(egui::Slider::new(&mut self.scope_gain_db, -24.0..=36.0)
                        .step_by(1.0).text(vertical_gain).suffix(" dB"));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(self.text("Übersicht unten ziehen oder horizontal scrollen", "Drag the overview below or scroll horizontally"));
                    if self.show_transients {
                        ui.add(egui::Slider::new(&mut self.transient_sensitivity, 0.4..=3.0)
                            .text(self.language.label_sensitivity()).suffix("×"));
                    }
                    if ui.button(self.language.btn_full_track_region()).clicked() {
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
                ui.horizontal(|ui| {
                    let play_label = if self.player.is_playing() {
                        self.text("Pause", "Pause")
                    } else {
                        self.language.btn_play_region()
                    };
                    if ui.button(play_label).clicked() {
                        self.player.toggle_range(active_region.0, active_region.1);
                    }

                    if ui.button("✂ Schnitt am Anfang").clicked() {
                        self.add_marker(active_region.0);
                    }

                    if ui.button("✂ Schnitt am Ende").clicked() {
                        self.add_marker(active_region.1);
                    }

                    if let Some((index, time)) = self.selected_marker_idx
                        .and_then(|i| self.splitter.markers.get(i).map(|&time| (i, time)))
                    {
                        let closest = transients.iter().min_by(|a, b| {
                            (a.time_sec - time).abs().total_cmp(&(b.time_sec - time).abs())
                        }).filter(|point| (point.time_sec - time).abs() < 2.0);
                        let snap = self.text("An Transiente einrasten", "Snap to transient");
                        if ui.add_enabled(closest.is_some(), egui::Button::new(snap)).clicked() {
                            self.splitter.move_marker(index, closest.unwrap().time_sec, total_sec, rate);
                        }
                    }
                });
            });
        }

        if let Some((cuts, gaps)) = self.pause_preview.as_ref().map(|p| (p.markers.len(), p.pause_indices.len())) {
            ui.group(|ui| {
                ui.label(format!("{}: {} · {}: {}", self.text("Vorgeschlagene Schnitte", "Suggested cuts"),
                    cuts, self.text("Pausen", "Gaps"), gaps));
                ui.label(self.text(
                    "Gestrichelte Linien zeigen die Vorschläge. Übernehmen ersetzt die Trackliste und ist rückgängig machbar.",
                    "Dashed lines show the suggestions. Apply replaces the track list and can be undone.",
                ));
                ui.horizontal(|ui| {
                    if ui.button(self.text("Übernehmen", "Apply")).clicked() {
                        let before = self.splitter.snapshot();
                        self.splitter.restore(self.pause_preview.take().unwrap());
                        self.splitter.commit_edit(before);
                        self.reset_editor_selection();
                    }
                    if ui.button(self.text("Verwerfen", "Discard")).clicked() {
                        self.pause_preview = None;
                    }
                });
            });
        }

        // 3. Delete Confirmation Banner (if pending)
        if let Some((idx, sec, ref title)) = self.pending_delete_marker.clone() {
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(58, 24, 24))
                .stroke(Stroke::new(1.5, LED_RED))
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mins = (sec / 60.0).floor() as u64;
                        let secs = (sec % 60.0).floor() as u64;
                        ui.label(RichText::new("⚠️").size(18.0));
                        ui.label(
                            RichText::new(self.language.confirm_delete_question(mins, secs, &title))
                                .strong()
                                .color(Color32::WHITE),
                        );

                        ui.add_space(12.0);

                        let confirm_btn = egui::Button::new(
                            RichText::new(self.language.btn_confirm_delete()).strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(180, 30, 30))
                        .rounding(4.0);
                        if ui.add(confirm_btn).clicked() {
                            if self.splitter.markers.get(idx) == Some(&sec) {
                                self.splitter.remove_marker_at_index(idx);
                                self.selected_marker_idx = None;
                                self.selected_segment_idx = None;
                            }
                            self.pending_delete_marker = None;
                        }

                        if ui.button(self.language.btn_cancel()).clicked() {
                            self.pending_delete_marker = None;
                        }
                    });
                });
        }

        ui.add_space(10.0);

        // 4. Tracklist Table with Pause Exclusions, Responsive Title Input & Nudge Controls
        let tracks = self.splitter.get_tracks(total_sec);

        while self.splitter.custom_titles.len() < tracks.len() {
            self.splitter.custom_titles.push(String::new());
        }

        if self.show_tracks {
            render_hardware_rack_card(ui, &self.language.card_tracklist(tracks.len()), |ui| {
                egui::Grid::new("track_list").striped(true).min_col_width(40.0).spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        for title in ["#", self.language.col_title(), self.language.col_start(),
                            self.language.col_duration(), self.language.col_actions()] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for (i, track) in tracks.iter().enumerate() {
                            let selected = self.selected_segment_idx == Some(i);
                            let badge = if track.is_pause { "⏸".into() } else { format!("{:02}", track.index) };
                            if ui.selectable_label(selected, badge).clicked() {
                                self.selected_segment_idx = Some(i);
                                self.selected_marker_idx = i.checked_sub(1);
                                self.player.seek_seconds(track.start_seconds);
                            }
                            if track.is_pause {
                                ui.label(self.language.badge_pause());
                            } else {
                                let response = ui.add(egui::TextEdit::singleline(&mut self.splitter.custom_titles[i])
                                    .id_source(egui::Id::new("track_title_field").with(i))
                                    .desired_width((ui.available_width() * 0.31).clamp(150.0, 360.0))
                                    .hint_text(format!("Track {:02}", track.index)));
                                if response.gained_focus() {
                                    self.selected_segment_idx = Some(i);
                                    self.selected_marker_idx = i.checked_sub(1);
                                }
                                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                    if let Some(next) = tracks.iter().skip(i + 1).find(|track| !track.is_pause) {
                                        ui.memory_mut(|mem| mem.request_focus(egui::Id::new("track_title_field").with(next.segment_index)));
                                    }
                                }
                            }
                            if i > 0 {
                                if ui.selectable_label(selected, RichText::new(timecode(track.start_seconds)).monospace()).clicked() {
                                    self.select_marker(i - 1);
                                    self.show_inspector = true;
                                }
                            } else {
                                ui.monospace(timecode(track.start_seconds));
                            }
                            ui.monospace(track.duration_formatted());
                            ui.horizontal(|ui| {
                                if ui.small_button(self.text("Anhören", "Audition")).clicked() {
                                    self.player.play_range(track.start_seconds, track.end_seconds);
                                }
                                let action = if track.is_pause { self.language.btn_make_track() } else { self.language.btn_make_pause() };
                                if ui.small_button(action).clicked() {
                                    self.splitter.toggle_pause(i);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        }

        self.render_export_dialog(ui, tracks, has_audio);
    }

    fn render_export_dialog(&mut self, ui: &mut Ui, tracks: Vec<crate::splitter::TrackInfo>, has_audio: bool) {
        if !self.show_export {
            return;
        }
        let mut show_export = true;
        egui::Window::new(self.text("Tracks exportieren", "Export tracks"))
            .id(egui::Id::new("export_dialog"))
            .open(&mut show_export).collapsible(false).resizable(true)
            .default_width(550.0).show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.language.label_export_dir()).color(TEXT_SECONDARY));
                ui.label(RichText::new(self.export_directory.display().to_string()).monospace().color(TEXT_PRIMARY));
                if ui.button(self.language.btn_choose_folder()).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.export_directory = folder;
                        self.save_config();
                    }
                }
            });

            ui.add_space(4.0);

            let effective_dir = self.effective_export_directory();
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.language.label_export_target_preview()).color(LED_AMBER));
                ui.label(RichText::new(effective_dir.display().to_string()).monospace().color(Color32::WHITE).strong());
            });

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new(self.language.label_normalization()).color(TEXT_SECONDARY));
                if ui.radio_value(&mut self.normalization_mode, NormalizationMode::None, self.language.norm_original()).changed()
                    || ui.radio_value(&mut self.normalization_mode, NormalizationMode::PeakPerTrack, self.language.norm_per_track()).changed()
                    || ui.radio_value(&mut self.normalization_mode, NormalizationMode::PeakAlbum, self.language.norm_album()).changed()
                {
                    self.save_config();
                }
            });

            ui.add_space(12.0);

            let files: Vec<_> = tracks.iter().filter(|track| !track.is_pause).map(|track| {
                let title = crate::project::sanitize_track_title(&track.title, track.index);
                format!("{:02} - {title}.wav", track.index)
            }).collect();
            let active_count = files.len();
            let conflict = files.iter().any(|name| effective_dir.join(name).exists());
            ui.label(format!("{}: {}", self.text("Ausgelassene Pausen", "Excluded gaps"), tracks.len() - active_count));
            egui::ScrollArea::vertical().max_height(175.0).show(ui, |ui| {
                for name in &files {
                    if effective_dir.join(name).exists() {
                        ui.colored_label(LED_RED, format!("{} · {name}", self.text("Existiert bereits", "Already exists")));
                    } else {
                        ui.label(name);
                    }
                }
            });
            let export_enabled = has_audio && active_count > 0 && !conflict && self.job.is_none();
            let export_btn = egui::Button::new(
                RichText::new(self.language.btn_export_tracks(active_count))
                    .size(15.0)
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(if export_enabled { Color32::from_rgb(37, 99, 235) } else { Color32::from_rgb(50, 50, 58) })
            .stroke(Stroke::new(1.0, if export_enabled { Color32::from_rgb(96, 165, 250) } else { BORDER_MID }))
            .rounding(6.0)
            .min_size(Vec2::new(380.0, 42.0));

            if ui.add_enabled(export_enabled, export_btn).clicked() {
                if let Some(source) = self.current_audio_path.clone() {
                    let target_dir = self.effective_export_directory();
                    let mode = self.normalization_mode.clone();
                    self.export_status_message = None;
                    let progress = Arc::new(ExportProgress::default());
                    let worker_progress = progress.clone();
                    self.start_job(ui.ctx(), self.text("Tracks werden exportiert …", "Exporting tracks …"), move || {
                        let files = export_tracks(&source, &target_dir, &tracks, mode, |finished, total| {
                            worker_progress.report(finished, total)
                        })?;
                        Ok(JobResult::Export(files.len(), target_dir))
                    });
                    if let Some(job) = &mut self.job {
                        job.export_progress = Some(progress);
                    }
                }
            }

            if let Some((is_success, ref msg)) = self.export_status_message {
                ui.add_space(8.0);
                let col = if is_success { LED_GREEN } else { LED_RED };
                ui.horizontal(|ui| {
                    ui.colored_label(col, msg);
                    let target_dir = self.effective_export_directory();
                    if is_success && ui.button(self.language.btn_show_in_finder()).clicked() {
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&target_dir).spawn();
                    }
                });
            }
        });
        self.show_export = show_export;
    }
}

fn render_hardware_rack_card<F: FnOnce(&mut Ui)>(ui: &mut Ui, rack_title: &str, content: F) {
    egui::Frame::none()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_DARK))
        .rounding(6.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.label(RichText::new(rack_title).size(13.0).strong().color(TEXT_SECONDARY));
            ui.add_space(8.0);
            content(ui);
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
            waveform_height: 280.0,
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
            hover_is_auditioning: false,
            hover_audition_time: None,
            export_directory: PathBuf::from("/tmp"),
            normalization_mode: NormalizationMode::None,
            export_status_message: None,
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
    }
}

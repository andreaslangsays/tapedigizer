use std::path::{Path, PathBuf};
use std::time::Duration;
use egui::{Color32, Pos2, Rect, RichText, Stroke, Ui, Vec2};
use crate::analysis::{analyze_wav_file, AudioAnalysisReport, LevelDiagnosis};
use crate::audio::device::{get_channel_pairs_for_device, list_input_devices, AudioDeviceInfo, ChannelPair};
use crate::audio::player::AudioPlayer;
use crate::audio::recorder::{AudioRecorder, RecorderState, StereoWaveformPoint};
use crate::splitter::{export_tracks, NormalizationMode, TrackSplitter};
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
    waveform_cache: Vec<StereoWaveformPoint>,
    selected_marker_idx: Option<usize>,
    selected_segment_idx: Option<usize>,
    waveform_zoom: f32,
    waveform_vertical_zoom: f32,
    waveform_height: f32,
    magnifier_scope_active: bool,
    selected_region: Option<(f64, f64)>,
    transient_sensitivity: f32,
    show_transients: bool,
    scope_zoom: f32,
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
            waveform_cache: Vec::new(),
            selected_marker_idx: None,
            selected_segment_idx: None,
            waveform_zoom: 1.0,
            waveform_vertical_zoom: 1.0,
            waveform_height: 280.0,
            magnifier_scope_active: false,
            selected_region: None,
            transient_sensitivity: 1.0,
            show_transients: true,
            scope_zoom: 1.0,
            scope_scroll: 0.0,
            waveform_scroll: 0.0,
            active_edit_tool: EditTool::Pointer,
            hover_preview_enabled: true,
            hover_mouse_pos: None,
            hover_stationary_since: None,
            hover_is_auditioning: false,
            hover_audition_time: None,
            export_directory: default_export_dir,
            normalization_mode: norm_mode,
            export_status_message: None,
            ui_error_message: None,
        };

        app.save_config();
        app
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

    pub fn current_recording_path(&self) -> PathBuf {
        let session_dir = self.session_project_dir();
        let _ = std::fs::create_dir_all(&session_dir);
        let side_str = self.tape_side.folder_name(Language::En);
        let filename = format!("{}_{}.wav", sanitize_filename(&self.tape_name), side_str);
        session_dir.join(filename)
    }

    pub fn save_project_quick(&mut self) {
        if let Some(path) = self.current_project_path.clone() {
            self.save_project_to_path(&path);
        } else {
            self.save_project_dialog();
        }
    }

    pub fn save_project_to_path(&mut self, path: &Path) {
        let norm_str = match self.normalization_mode {
            NormalizationMode::None => "none",
            NormalizationMode::PeakPerTrack => "track",
            NormalizationMode::PeakAlbum => "album",
        };
        let rec_wav = self.current_recording_path();
        let temp_wav = self.temp_record_path();
        let session_wav = if let Some(ref p) = self.current_audio_path {
            if p.exists() {
                Some(p.as_path())
            } else if rec_wav.exists() {
                Some(rec_wav.as_path())
            } else if temp_wav.exists() {
                Some(temp_wav.as_path())
            } else {
                None
            }
        } else if rec_wav.exists() {
            Some(rec_wav.as_path())
        } else if temp_wav.exists() {
            Some(temp_wav.as_path())
        } else {
            None
        };

        let proj = TapeDigizerProject::new(
            self.tape_name.clone(),
            self.tape_side,
            session_wav.and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy().to_string(),
            self.splitter.markers.clone(),
            self.splitter.custom_titles.clone(),
            self.splitter.pause_indices.clone(),
            norm_str.to_string(),
        );

        match proj.save_to_file(path, session_wav) {
            Ok(saved_path) => {
                self.current_project_path = Some(saved_path.clone());
                let msg = if self.language == Language::De {
                    format!("Projekt gespeichert: {}", saved_path.file_name().unwrap_or_default().to_string_lossy())
                } else {
                    format!("Project saved: {}", saved_path.file_name().unwrap_or_default().to_string_lossy())
                };
                self.export_status_message = Some((true, msg));
            }
            Err(e) => {
                let err = if self.language == Language::De {
                    format!("Fehler beim Speichern: {}", e)
                } else {
                    format!("Error saving project: {}", e)
                };
                self.export_status_message = Some((false, err));
            }
        }
    }

    pub fn new_project(&mut self) {
        self.player.stop();
        self.recorder.stop_monitoring();
        self.recorder.stop_recording();
        self.tape_name = if self.language == Language::De { "Neues Tonband".into() } else { "New Tape".into() };
        self.tape_side = TapeSide::SideA;
        self.current_project_path = None;
        self.current_audio_path = None;
        self.analysis_report = None;
        self.waveform_cache.clear();
        self.splitter.clear_markers();
        self.splitter.custom_titles.clear();
        self.splitter.pause_indices.clear();
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

    pub fn save_project_dialog(&mut self) {
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
            self.save_project_to_path(&path);
        }
    }

    pub fn open_project_dialog(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("TapeDigizer Project", &["tapedigizer"]);

        if let Some(path) = dialog.pick_file() {
            match TapeDigizerProject::load_from_file(&path) {
                Ok((proj, audio_path)) => {
                    self.tape_name = proj.tape_name;
                    self.tape_side = proj.tape_side;
                    self.splitter.markers = proj.markers;
                    self.splitter.custom_titles = proj.custom_titles;
                    self.splitter.pause_indices = proj.pause_indices;
                    self.normalization_mode = match proj.normalization_mode.as_str() {
                        "track" => NormalizationMode::PeakPerTrack,
                        "album" => NormalizationMode::PeakAlbum,
                        _ => NormalizationMode::None,
                    };
                    self.current_project_path = Some(path);
                    self.current_audio_path = Some(audio_path.clone());

                    if let Err(e) = self.player.load_file(&audio_path) {
                        eprintln!("Fehler beim Laden im Player: {}", e);
                    }
                    self.waveform_cache = generate_stereo_waveform_from_samples(
                        &self.player.get_samples_l(),
                        &self.player.get_samples_r(),
                        self.player.total_samples(),
                    );
                    self.active_tab = ActiveTab::SplitAndExport;

                    let msg = if self.language == Language::De {
                        format!("Projekt '{}' geladen!", self.tape_name)
                    } else {
                        format!("Project '{}' loaded!", self.tape_name)
                    };
                    self.export_status_message = Some((true, msg));
                }
                Err(e) => {
                    let err = if self.language == Language::De {
                        format!("Fehler beim Laden: {}", e)
                    } else {
                        format!("Error loading project: {}", e)
                    };
                    self.export_status_message = Some((false, err));
                }
            }
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

    fn temp_record_path(&self) -> PathBuf {
        std::env::temp_dir().join("tapedigizer_session.wav")
    }

    fn finish_recording_and_analyze(&mut self) {
        let rec_path = self.current_recording_path();
        let temp_path = self.temp_record_path();
        let target_path = if let Some(ref p) = self.current_audio_path {
            if p.exists() {
                p.clone()
            } else if rec_path.exists() {
                rec_path
            } else if temp_path.exists() {
                temp_path
            } else {
                rec_path
            }
        } else if rec_path.exists() {
            rec_path
        } else if temp_path.exists() {
            temp_path
        } else {
            self.ui_error_message = Some(if self.language == Language::De {
                "Aufnahmedatei wurde nicht auf der Festplatte gefunden.".into()
            } else {
                "Recording file was not found on disk.".into()
            });
            self.active_tab = ActiveTab::Diagnosis;
            return;
        };

        if !target_path.exists() {
            self.ui_error_message = Some(if self.language == Language::De {
                "Aufnahmedatei wurde nicht auf der Festplatte gefunden.".into()
            } else {
                "Recording file was not found on disk.".into()
            });
            self.active_tab = ActiveTab::Diagnosis;
            return;
        }

        self.current_audio_path = Some(target_path.clone());

        // 1. VOR DER ANALYSE: Projekt sofort automatisch auf Festplatte sichern!
        let session_dir = self.session_project_dir();
        let _ = std::fs::create_dir_all(&session_dir);
        let auto_proj_filename = format!(
            "{}_{}.tapedigizer",
            sanitize_filename(&self.tape_name),
            self.tape_side.folder_name(Language::En)
        );
        let auto_proj_path = session_dir.join(&auto_proj_filename);
        let norm_str = match self.normalization_mode {
            NormalizationMode::None => "none",
            NormalizationMode::PeakPerTrack => "track",
            NormalizationMode::PeakAlbum => "album",
        };
        let proj = TapeDigizerProject::new(
            self.tape_name.clone(),
            self.tape_side,
            target_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            self.splitter.markers.clone(),
            self.splitter.custom_titles.clone(),
            self.splitter.pause_indices.clone(),
            norm_str.to_string(),
        );
        if let Ok(saved) = proj.save_to_file(&auto_proj_path, Some(&target_path)) {
            self.current_project_path = Some(saved);
        }

        // 2. Analyse durchführen
        match analyze_wav_file(&target_path) {
            Ok(report) => {
                self.analysis_report = Some(report);
                self.ui_error_message = None;
                self.active_tab = ActiveTab::Diagnosis;
            }
            Err(e) => {
                self.ui_error_message = Some(format!("Analyse-Fehler: {}", e));
                self.active_tab = ActiveTab::Diagnosis;
            }
        }

        // 3. Audio in Player laden
        if let Err(e) = self.player.load_file(&target_path) {
            eprintln!("Fehler beim Laden im Player: {}", e);
        }

        // 4. Echte Wellenform aus geladenen PCM-Samples generieren
        self.waveform_cache = generate_stereo_waveform_from_samples(
            &self.player.get_samples_l(),
            &self.player.get_samples_r(),
            self.player.total_samples(),
        );

        self.splitter.clear_markers();
    }
}

impl eframe::App for TapeDigizerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_recording = self.recorder.state() == RecorderState::Recording;
        let is_monitoring = self.recorder.state() == RecorderState::Monitoring;
        let is_playing = self.player.is_playing();

        if is_recording || is_monitoring || is_playing {
            ctx.request_repaint();
        }

        if self.recorder.poll_auto_stop() {
            self.finish_recording_and_analyze();
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

        if cmd && key_1 {
            self.active_tab = ActiveTab::Recording;
        }
        if cmd && key_2 {
            self.active_tab = ActiveTab::Diagnosis;
        }
        if cmd && key_3 {
            self.active_tab = ActiveTab::SplitAndExport;
        }

        if cmd && key_n {
            self.new_project();
        }
        if cmd && key_s {
            if shift {
                self.save_project_dialog();
            } else {
                self.save_project_quick();
            }
        }
        if cmd && key_o {
            self.open_project_dialog();
        }

        if !ctx.wants_keyboard_input() {
            if cmd && key_z {
                if shift {
                    self.splitter.redo();
                } else {
                    self.splitter.undo();
                }
            } else if cmd && key_y {
                self.splitter.redo();
            }
        }

        // Global Shortcuts in Tab 3 (Split & Export)
        let has_audio = self.player.total_seconds() > 0.1;
        if self.active_tab == ActiveTab::SplitAndExport && !ctx.wants_keyboard_input() {
            if self.pending_delete_marker.is_some() && key_esc {
                self.pending_delete_marker = None;
            }

            if has_audio && key_space {
                self.player.toggle_play();
            }
            if has_audio && key_m {
                let cur_sec = self.player.current_seconds();
                let total_sec = self.player.total_seconds();
                self.splitter.add_marker(cur_sec, total_sec);
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
                    self.splitter.nudge_marker(m_idx, -delta, total_sec);
                }
                if key_right {
                    self.splitter.nudge_marker(m_idx, delta, total_sec);
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
                if let Some(idx) = self.selected_marker_idx.take() {
                    let sec = self.splitter.markers.get(idx).copied().unwrap_or(0.0);
                    let title = self.splitter.custom_titles.get(idx + 1).cloned().unwrap_or_else(|| format!("Track {:02}", idx + 2));
                    self.pending_delete_marker = Some((idx, sec, title));
                }
            }
        }

        // Top Application Menu Bar (Ablage / File, Bearbeiten / Edit, etc.)
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
                            self.open_project_dialog();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(format!("{} (⌘S)", self.language.menu_save_project())).clicked() {
                            self.save_project_quick();
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (⇧⌘S)", self.language.menu_save_project_as())).clicked() {
                            self.save_project_dialog();
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
                            self.splitter.undo();
                            ui.close_menu();
                        }
                        if ui.button(format!("{} (⇧⌘Z)", self.language.menu_redo())).clicked() {
                            self.splitter.redo();
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(self.language.menu_help(), |ui| {
                        if ui.button(self.language.menu_about()).clicked() {
                            self.export_status_message = Some((true, "TapeDigizer v1.2.0 — High-fidelity Audio Digitization Console".into()));
                            ui.close_menu();
                        }
                    });
                });
            });

        // Top Studio Header (Matches macOS Dark Window Decoration)
        egui::TopBottomPanel::top("top_header")
            .frame(egui::Frame::none().fill(BG_WINDOW).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Vintage Hardware Badge with drawn vector cassette icon
                    let (cassette_rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 20.0), egui::Sense::hover());
                    draw_vector_cassette(ui.painter(), cassette_rect);

                    ui.add_space(4.0);
                    ui.label(RichText::new("TAPEDIGIZER").size(18.0).monospace().strong().color(LED_AMBER));
                    ui.label(RichText::new("PRO AUDIO TAPE CONSOLE").size(9.5).monospace().color(TEXT_MUTED));
                    ui.label(RichText::new("v1.2.0").size(9.0).monospace().color(Color32::from_rgb(90, 95, 110)));

                    // Right Side: Language Switcher + Logic Pro Inspired Center LCD Console
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Language switcher
                        let is_en = self.language == Language::En;
                        if ui.selectable_label(is_en, "EN").on_hover_text("English").clicked() && !is_en {
                            self.language = Language::En;
                            self.save_config();
                        }
                        let is_de = self.language == Language::De;
                        if ui.selectable_label(is_de, "DE").on_hover_text("Deutsch").clicked() && !is_de {
                            self.language = Language::De;
                            self.save_config();
                        }

                        ui.add_space(8.0);

                        let dev_text = if let Some(dev) = self.current_device() {
                            format!("{} ({} Ch @ {} Hz)", dev.name, dev.channels, dev.sample_rate)
                        } else {
                            self.language.label_no_device().into()
                        };

                        let lcd_frame = egui::Frame::none()
                            .fill(BG_LCD)
                            .stroke(Stroke::new(1.0, BORDER_DARK))
                            .rounding(4.0)
                            .inner_margin(Vec2::new(12.0, 5.0));

                        lcd_frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (status_led, status_lbl) = if is_recording {
                                    (LED_RED, "REC")
                                } else if is_monitoring {
                                    (LED_AMBER, "MONITOR")
                                } else {
                                    (LED_GREEN, "READY")
                                };

                                let (led_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                                ui.painter().circle_filled(led_rect.center(), 3.5, status_led);

                                ui.label(RichText::new(status_lbl).color(status_led).monospace().small().strong());
                                ui.label(RichText::new("|").color(BORDER_MID));
                                ui.label(RichText::new(dev_text).color(TEXT_SECONDARY).monospace().small());
                                ui.label(RichText::new("|").color(BORDER_MID));
                                ui.label(RichText::new("24-BIT PCM").color(Color32::from_rgb(56, 189, 248)).monospace().small());
                            });
                        });
                    });
                });

                ui.add_space(10.0);

                // Tactile Step Pills with clean lettering (localized)
                ui.horizontal(|ui| {
                    render_studio_tab_pill(ui, &mut self.active_tab, ActiveTab::Recording, self.language.tab_recording());
                    ui.add_space(8.0);
                    render_studio_tab_pill(ui, &mut self.active_tab, ActiveTab::Diagnosis, self.language.tab_diagnosis());
                    ui.add_space(8.0);
                    render_studio_tab_pill(ui, &mut self.active_tab, ActiveTab::SplitAndExport, self.language.tab_split_export());
                });
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

                match self.active_tab {
                    ActiveTab::Recording => self.render_tab_recording(ui),
                    ActiveTab::Diagnosis => self.render_tab_diagnosis(ui),
                    ActiveTab::SplitAndExport => self.render_tab_split_export(ui),
                }
            });
    }
}

fn render_studio_tab_pill(ui: &mut Ui, current: &mut ActiveTab, target: ActiveTab, label: &str) {
    let is_active = *current == target;
    let (bg, border, text_col) = if is_active {
        (
            Color32::from_rgb(37, 99, 235),
            Color32::from_rgb(96, 165, 250),
            Color32::WHITE,
        )
    } else {
        (
            Color32::from_rgb(46, 46, 52),
            BORDER_MID,
            TEXT_SECONDARY,
        )
    };

    let btn = egui::Button::new(RichText::new(label).size(13.0).strong().color(text_col))
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .rounding(5.0)
        .min_size(Vec2::new(170.0, 32.0));

    if ui.add(btn).clicked() {
        *current = target;
    }
}

impl TapeDigizerApp {
    // -------------------------------------------------------------
    // TAB 1: Aufnahme & Einpegeln
    // -------------------------------------------------------------
    fn render_tab_recording(&mut self, ui: &mut Ui) {
        ui.add_space(2.0);

        // Rack Panel 0: Cassette Metadata & Project Location
        render_hardware_rack_card(ui, self.language.card_tape_metadata(), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.language.label_tape_name()).color(TEXT_SECONDARY).strong());
                let name_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.tape_name)
                        .desired_width(380.0)
                        .hint_text(self.language.hint_tape_name())
                );
                if name_edit.changed() {
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
            ui.horizontal(|ui| {
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
        render_hardware_rack_card(ui, self.language.card_audio_routing(), |ui| {
            ui.horizontal(|ui| {
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

        // Rack Panel 2: Studio LED Ladder Peak/VU Meters
        render_hardware_rack_card(ui, self.language.card_led_meters(), |ui| {
            ui.horizontal(|ui| {
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

                    if ui.add(mon_btn).clicked() {
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
        render_hardware_rack_card(ui, self.language.card_recording_control(), |ui| {
            ui.horizontal(|ui| {
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

            let is_recording = self.recorder.state() == RecorderState::Recording;

            ui.horizontal(|ui| {
                if !is_recording {
                    // Tactile Ruby Red Studio Push-button with clean bold lettering
                    let btn = egui::Button::new(
                        RichText::new(self.language.btn_start_recording())
                            .size(15.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(Color32::from_rgb(197, 40, 40))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(255, 100, 100)))
                    .rounding(6.0)
                    .min_size(Vec2::new(340.0, 48.0));

                    if ui.add(btn).clicked() {
                        let dev_name = self.current_device().map(|d| d.name.clone());
                        let pair = self.current_channel_pair();
                        if let (Some(name), Some(pair)) = (dev_name, pair) {
                            let timer_dur = if self.selected_timer_minutes > 0 {
                                Some(Duration::from_secs(self.selected_timer_minutes as u64 * 60))
                            } else {
                                None
                            };

                            let target_path = self.current_recording_path();
                            self.current_audio_path = Some(target_path.clone());
                            let silence_stop = self.enable_silence_autostop;
                            if let Err(e) = self.recorder.start_recording(
                                &name,
                                pair,
                                target_path,
                                timer_dur,
                                silence_stop,
                            ) {
                                self.ui_error_message = Some(e);
                            }
                        }
                    }
                } else {
                    let btn = egui::Button::new(
                        RichText::new(self.language.btn_stop_recording())
                            .size(15.0)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(Color32::from_rgb(217, 83, 30))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(251, 146, 60)))
                    .rounding(6.0)
                    .min_size(Vec2::new(340.0, 48.0));

                    if ui.add(btn).clicked() {
                        self.recorder.stop_recording();
                        self.finish_recording_and_analyze();
                    }
                }

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
                        ui.horizontal(|ui| {
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
        render_hardware_rack_card(ui, self.language.card_live_waveform(), |ui| {
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
        let has_audio = total_sec > 0.1 && !self.waveform_cache.is_empty();

        // 1. Tape Project & Side Selector Bar
        render_hardware_rack_card(ui, self.language.card_tape_project(), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.language.label_tape_name()).color(TEXT_SECONDARY).strong());
                let name_edit = ui.add(
                    egui::TextEdit::singleline(&mut self.tape_name)
                        .desired_width(380.0)
                        .hint_text(self.language.hint_tape_name())
                );
                if name_edit.changed() {
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(self.language.btn_open_project()).clicked() {
                        self.open_project_dialog();
                    }
                    if ui.button(self.language.btn_save_project()).clicked() {
                        self.save_project_dialog();
                    }
                });
            });
        });

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
        render_hardware_rack_card(ui, self.language.card_arrange(), |ui| {
            // Header Bar: Tool Modes, Undo/Redo, Hover Audition, Timecode
            ui.horizontal(|ui| {
                // Mode 1: Pointer Tool
                let is_pointer = self.active_edit_tool == EditTool::Pointer;
                let ptr_btn = egui::Button::new(
                    RichText::new(self.language.btn_tool_pointer())
                        .size(11.5)
                        .strong()
                        .color(if is_pointer { Color32::WHITE } else { TEXT_SECONDARY }),
                )
                .fill(if is_pointer { Color32::from_rgb(37, 99, 235) } else { Color32::from_rgb(38, 38, 44) })
                .stroke(Stroke::new(1.0, if is_pointer { Color32::from_rgb(96, 165, 250) } else { BORDER_MID }))
                .rounding(4.0);

                if ui.add(ptr_btn).clicked() {
                    self.active_edit_tool = EditTool::Pointer;
                }

                // Mode 2: Scissor Tool
                let is_scissor = self.active_edit_tool == EditTool::Scissor;
                let cut_btn = egui::Button::new(
                    RichText::new(self.language.btn_tool_scissor())
                        .size(11.5)
                        .strong()
                        .color(if is_scissor { Color32::BLACK } else { LED_AMBER }),
                )
                .fill(if is_scissor { LED_AMBER } else { Color32::from_rgb(44, 38, 25) })
                .stroke(Stroke::new(1.0, if is_scissor { Color32::WHITE } else { LED_AMBER }))
                .rounding(4.0);

                if ui.add(cut_btn).clicked() {
                    self.active_edit_tool = if is_scissor { EditTool::Pointer } else { EditTool::Scissor };
                }

                // Mode 3: Select Tool (Bereichsauswahl)
                let is_select = self.active_edit_tool == EditTool::Select;
                let sel_btn = egui::Button::new(
                    RichText::new(self.language.btn_tool_select())
                        .size(11.5)
                        .strong()
                        .color(if is_select { Color32::WHITE } else { Color32::from_rgb(255, 215, 0) }),
                )
                .fill(if is_select { Color32::from_rgb(180, 130, 20) } else { Color32::from_rgb(44, 38, 25) })
                .stroke(Stroke::new(1.0, if is_select { Color32::WHITE } else { Color32::from_rgb(200, 160, 40) }))
                .rounding(4.0);

                if ui.add(sel_btn).clicked() {
                    self.active_edit_tool = if is_select { EditTool::Pointer } else { EditTool::Select };
                }

                ui.add_space(8.0);

                // Mode 3: Hover Audition Toggle
                let is_hov = self.hover_preview_enabled;
                let hov_text = if is_hov { self.language.btn_hover_on() } else { self.language.btn_hover_off() };
                let hov_btn = egui::Button::new(
                    RichText::new(hov_text)
                        .size(11.5)
                        .strong()
                        .color(if is_hov { Color32::WHITE } else { TEXT_MUTED }),
                )
                .fill(if is_hov { Color32::from_rgb(22, 101, 52) } else { Color32::from_rgb(36, 36, 42) })
                .stroke(Stroke::new(1.0, if is_hov { LED_GREEN } else { BORDER_MID }))
                .rounding(4.0);

                if ui.add(hov_btn).clicked() {
                    self.hover_preview_enabled = !self.hover_preview_enabled;
                    if !self.hover_preview_enabled && self.hover_is_auditioning {
                        self.player.pause();
                        self.hover_is_auditioning = false;
                        self.hover_audition_time = None;
                    }
                }

                ui.add_space(8.0);

                // Mode 4: Magnifier Scope Toggle
                let is_mag = self.magnifier_scope_active;
                let mag_btn = egui::Button::new(
                    RichText::new(self.language.btn_magnifier_toggle())
                        .size(11.5)
                        .strong()
                        .color(if is_mag { Color32::BLACK } else { Color32::from_rgb(255, 215, 0) }),
                )
                .fill(if is_mag { Color32::from_rgb(255, 215, 0) } else { Color32::from_rgb(44, 38, 25) })
                .stroke(Stroke::new(1.0, if is_mag { Color32::WHITE } else { Color32::from_rgb(200, 160, 40) }))
                .rounding(4.0);

                if ui.add(mag_btn).clicked() {
                    self.magnifier_scope_active = !self.magnifier_scope_active;
                }

                ui.add_space(10.0);

                // Undo / Redo buttons
                let can_undo = self.splitter.can_undo();
                let undo_btn = egui::Button::new(
                    RichText::new("↶ Undo").size(11.0).color(if can_undo { Color32::WHITE } else { Color32::from_rgb(90, 95, 105) })
                )
                .rounding(4.0);
                if ui.add_enabled(can_undo, undo_btn).on_hover_text(self.language.btn_undo()).clicked() {
                    self.splitter.undo();
                }

                let can_redo = self.splitter.can_redo();
                let redo_btn = egui::Button::new(
                    RichText::new("↷ Redo").size(11.0).color(if can_redo { Color32::WHITE } else { Color32::from_rgb(90, 95, 105) })
                )
                .rounding(4.0);
                if ui.add_enabled(can_redo, redo_btn).on_hover_text(self.language.btn_redo()).clicked() {
                    self.splitter.redo();
                }

                // Timecode Display
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if has_audio {
                        let cur_m = (cur_sec / 60.0).floor() as u64;
                        let cur_s = (cur_sec % 60.0).floor() as u64;
                        let cur_ms = ((cur_sec % 1.0) * 10.0).floor() as u64;

                        let tot_m = (total_sec / 60.0).floor() as u64;
                        let tot_s = (total_sec % 60.0).floor() as u64;

                        ui.label(RichText::new(format!("{:02}:{:02}.{} / {:02}:{:02}", cur_m, cur_s, cur_ms, tot_m, tot_s)).size(15.0).monospace().strong().color(Color32::WHITE));
                    } else {
                        ui.label(RichText::new("--:--.- / --:--").size(15.0).monospace().strong().color(TEXT_MUTED));
                    }
                });
            });

            ui.add_space(2.0);
            ui.label(
                RichText::new(self.language.tip_navigation())
                    .size(11.0)
                    .color(TEXT_MUTED),
            );

            ui.add_space(6.0);

            let interaction = render_stereo_waveform(
                ui,
                &self.waveform_cache,
                total_sec,
                cur_sec,
                &self.splitter.markers,
                &self.splitter.pause_indices,
                &mut self.selected_marker_idx,
                self.selected_region,
                &mut self.waveform_zoom,
                &mut self.waveform_vertical_zoom,
                self.waveform_height,
                &mut self.waveform_scroll,
                self.active_edit_tool,
                self.hover_is_auditioning,
                self.hover_audition_time,
            );

            if let Some((r_start, r_end)) = interaction.new_selected_region {
                self.selected_region = Some((r_start, r_end));
                self.magnifier_scope_active = true;
            }
            if let Some(target_sec) = interaction.seek_to_seconds {
                if has_audio {
                    self.player.seek_seconds(target_sec);
                }
            }
            if let Some(new_marker_sec) = interaction.add_marker_at_seconds {
                if has_audio && total_sec > 1.0 {
                    self.splitter.add_marker(new_marker_sec, total_sec);
                }
            }
            if let Some((idx, new_time)) = interaction.dragged_marker {
                if idx < self.splitter.markers.len() && has_audio && total_sec > 1.0 {
                    self.splitter.push_undo();
                    self.splitter.markers[idx] = new_time.clamp(0.5, (total_sec - 0.5).max(0.5));
                    self.splitter.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
                }
            }
            if let Some(del_idx) = interaction.delete_marker_idx {
                let del_sec = self.splitter.markers.get(del_idx).copied().unwrap_or(0.0);
                let del_title = self.splitter.custom_titles.get(del_idx + 1).cloned().unwrap_or_else(|| format!("Track {:02}", del_idx + 2));
                self.pending_delete_marker = Some((del_idx, del_sec, del_title));
            }
            if let Some(seg_idx) = interaction.clicked_segment_idx {
                self.selected_segment_idx = Some(seg_idx);
                let tracks = self.splitter.get_tracks(total_sec);
                if let Some(t) = tracks.get(seg_idx) {
                    if self.active_edit_tool == EditTool::Select {
                        self.selected_region = Some((t.start_seconds, t.end_seconds));
                        self.magnifier_scope_active = true;
                    }
                    self.player.seek_seconds(t.start_seconds);
                }
            }

            // Hover Audition Engine
            if self.hover_preview_enabled && has_audio {
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
                        ui.ctx().request_repaint();
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

            // Transport & Waveform Zoom / Height Toolbar
            ui.horizontal(|ui| {
                let is_playing = self.player.is_playing();
                let (play_text, play_col, play_stroke) = if is_playing {
                    (self.language.btn_pause(), Color32::from_rgb(217, 83, 30), Color32::from_rgb(251, 146, 60))
                } else {
                    (self.language.btn_play(), Color32::from_rgb(37, 99, 235), Color32::from_rgb(96, 165, 250))
                };

                let play_btn = egui::Button::new(RichText::new(play_text).strong().size(13.0).color(Color32::WHITE))
                    .fill(if has_audio { play_col } else { Color32::from_rgb(45, 45, 52) })
                    .stroke(Stroke::new(1.0, if has_audio { play_stroke } else { BORDER_MID }))
                    .rounding(5.0)
                    .min_size(Vec2::new(110.0, 32.0));

                if ui.add_enabled(has_audio, play_btn).clicked() {
                    self.player.toggle_play();
                }

                if ui.add_enabled(has_audio, egui::Button::new(self.language.btn_stop())).clicked() {
                    self.player.stop();
                }

                ui.add_space(8.0);

                let add_m_btn = egui::Button::new(RichText::new(self.language.btn_add_marker()).color(LED_AMBER).strong())
                    .rounding(5.0);
                if ui.add_enabled(has_audio, add_m_btn).clicked() {
                    self.splitter.add_marker(cur_sec, total_sec);
                }

                if ui.add_enabled(has_audio, egui::Button::new(self.language.btn_auto_detect_pauses())).clicked() {
                    let samples_l = self.player.get_samples_l();
                    let samples_r = self.player.get_samples_r();
                    let rate = self.player.sample_rate();
                    self.splitter.detect_silence_pauses(&samples_l, &samples_r, rate, -45.0, 1.8);
                }

                let gap_btn = egui::Button::new(RichText::new(self.language.btn_auto_detect_pauses_gaps()).color(Color32::from_rgb(251, 146, 60)));
                if ui.add_enabled(has_audio, gap_btn).on_hover_text("Erkennt Stillephasen und markiert sie automatisch als weggelassene Pausen").clicked() {
                    let samples_l = self.player.get_samples_l();
                    let samples_r = self.player.get_samples_r();
                    let rate = self.player.sample_rate();
                    self.splitter.detect_silence_pauses_as_gaps(&samples_l, &samples_r, rate, -45.0, 1.8);
                }

                if ui.add_enabled(has_audio, egui::Button::new(self.language.btn_clear_markers())).clicked() {
                    self.splitter.clear_markers();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("1x").on_hover_text("Reset Zoom").clicked() {
                        self.waveform_zoom = 1.0;
                        self.waveform_vertical_zoom = 1.0;
                        self.waveform_scroll = 0.0;
                    }

                    // Vertical amplitude zoom (Pegel-Lupe)
                    ui.add(egui::Slider::new(&mut self.waveform_vertical_zoom, 1.0..=16.0).text(self.language.label_vertical_zoom()).suffix("x"));

                    ui.add_space(6.0);
                    // Waveform height presets
                    ui.label(RichText::new(self.language.label_waveform_height()).color(TEXT_SECONDARY).small());
                    let _ = ui.selectable_value(&mut self.waveform_height, 220.0, "220")
                        | ui.selectable_value(&mut self.waveform_height, 300.0, "300")
                        | ui.selectable_value(&mut self.waveform_height, 420.0, "420");

                    ui.add_space(6.0);
                    // Horizontal time zoom
                    ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0).text("H-Zoom").suffix("x"));
                });
            });
        });

        // 3. Detail-Lupenansicht (Magnifier Scope & Transient Analysis) when toggled
        if self.magnifier_scope_active && has_audio {
            ui.add_space(8.0);
            render_hardware_rack_card(ui, self.language.card_magnifier_scope(), |ui| {
                let active_region = self.selected_region.unwrap_or_else(|| {
                    if let Some(m_idx) = self.selected_marker_idx {
                        let m_time = self.splitter.markers.get(m_idx).copied().unwrap_or(cur_sec);
                        ((m_time - 1.5).max(0.0), (m_time + 1.5).min(total_sec))
                    } else if let Some(s_idx) = self.selected_segment_idx {
                        let tracks = self.splitter.get_tracks(total_sec);
                        if let Some(t) = tracks.get(s_idx) {
                            (t.start_seconds, t.end_seconds)
                        } else {
                            ((cur_sec - 1.5).max(0.0), (cur_sec + 1.5).min(total_sec))
                        }
                    } else {
                        ((cur_sec - 1.5).max(0.0), (cur_sec + 1.5).min(total_sec))
                    }
                });

                let samples_l = self.player.get_samples_l();
                let samples_r = self.player.get_samples_r();
                let rate = self.player.sample_rate();

                let transients = crate::splitter::detect_transients_in_range(
                    &samples_l,
                    &samples_r,
                    rate,
                    active_region.0,
                    active_region.1,
                    self.transient_sensitivity,
                );

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

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Scope Zoom Slider
                        ui.add(egui::Slider::new(&mut self.scope_zoom, 1.0..=50.0).text("🔍 Lupe-Zoom").suffix("x"));

                        ui.add_space(4.0);
                        if self.show_transients {
                            ui.add(egui::Slider::new(&mut self.transient_sensitivity, 0.4..=3.0).text(self.language.label_sensitivity()).suffix("x"));
                        }

                        if ui.button(self.language.btn_full_track_region()).clicked() {
                            let tracks = self.splitter.get_tracks(total_sec);
                            for t in tracks {
                                if cur_sec >= t.start_seconds && cur_sec <= t.end_seconds {
                                    self.selected_region = Some((t.start_seconds, t.end_seconds));
                                    break;
                                }
                            }
                        }
                        if ui.button("3s Fokus").clicked() {
                            self.selected_region = Some(((cur_sec - 1.5).max(0.0), (cur_sec + 1.5).min(total_sec)));
                        }
                    });
                });

                ui.add_space(4.0);

                let scope_action = crate::ui::waveform_widget::render_detail_magnifier_scope(
                    ui,
                    &self.waveform_cache,
                    &samples_l,
                    &samples_r,
                    rate,
                    total_sec,
                    active_region,
                    &self.splitter.markers,
                    self.selected_marker_idx,
                    self.waveform_vertical_zoom,
                    &transients,
                    self.show_transients,
                    self.scope_zoom,
                    &mut self.scope_scroll,
                    cur_sec,
                );

                if let Some(t_seek) = scope_action.seek_to_time {
                    self.player.seek_seconds(t_seek);
                }
                if let Some(t_trans) = scope_action.clicked_transient_time {
                    self.splitter.add_marker(t_trans, total_sec);
                    self.player.seek_seconds(t_trans);
                }
                if let Some(s_idx) = scope_action.selected_marker_idx {
                    self.selected_marker_idx = Some(s_idx);
                }
                if let Some((idx, new_t)) = scope_action.dragged_marker {
                    if idx < self.splitter.markers.len() && has_audio && total_sec > 1.0 {
                        self.splitter.push_undo();
                        self.splitter.markers[idx] = new_t.clamp(0.01, (total_sec - 0.01).max(0.01));
                        self.splitter.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    }
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let play_reg_btn = egui::Button::new(RichText::new(self.language.btn_play_region()).color(Color32::WHITE).strong())
                        .fill(Color32::from_rgb(37, 99, 235))
                        .rounding(4.0);
                    if ui.add(play_reg_btn).clicked() {
                        self.player.play_range(active_region.0, active_region.1);
                    }

                    if ui.button("✂ Schnitt am Anfang").clicked() {
                        if active_region.0 > 0.1 && active_region.0 < total_sec - 0.1 {
                            self.splitter.add_marker(active_region.0, total_sec);
                        }
                    }

                    if ui.button("✂ Schnitt am Ende").clicked() {
                        if active_region.1 > 0.1 && active_region.1 < total_sec - 0.1 {
                            self.splitter.add_marker(active_region.1, total_sec);
                        }
                    }

                    // Auto-target marker in active region if none selected
                    let target_marker_idx = self.selected_marker_idx.or_else(|| {
                        self.splitter.markers.iter().position(|&m| m >= active_region.0 && m <= active_region.1)
                    });

                    if let Some(m_idx) = target_marker_idx {
                        let m_time = self.splitter.markers.get(m_idx).copied().unwrap_or(0.0);
                        let m_title = self.splitter.custom_titles.get(m_idx + 1).cloned().unwrap_or_else(|| format!("Track {:02}", m_idx + 2));
                        let sample_pos = (m_time * rate as f64).round() as u64;

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("📍 T{:02} ({:.4}s | Smp: {}):", m_idx + 2, m_time, sample_pos))
                                .color(LED_AMBER)
                                .monospace()
                                .strong(),
                        );

                        // Sample-accurate & Millisecond Nudge Micro-Controls
                        let smp_sec = 1.0 / rate.max(1) as f64;

                        if ui.button("◀ -100ms").on_hover_text("-100 Millisekunden").clicked() {
                            self.splitter.nudge_marker(m_idx, -0.1, total_sec);
                        }
                        if ui.button("◀ -10ms").on_hover_text("-10 Millisekunden").clicked() {
                            self.splitter.nudge_marker(m_idx, -0.01, total_sec);
                        }
                        if ui.button("◀ -1ms").on_hover_text("-1 Millisekunde").clicked() {
                            self.splitter.nudge_marker(m_idx, -0.001, total_sec);
                        }
                        if ui.button("◀ -1 Smp").on_hover_text(format!("Samplegenau: 1 Sample zurück ({:.6}s)", smp_sec)).clicked() {
                            self.splitter.nudge_marker(m_idx, -smp_sec, total_sec);
                        }
                        if ui.button("+1 Smp ▶").on_hover_text(format!("Samplegenau: 1 Sample vor ({:.6}s)", smp_sec)).clicked() {
                            self.splitter.nudge_marker(m_idx, smp_sec, total_sec);
                        }
                        if ui.button("+1ms ▶").on_hover_text("+1 Millisekunde").clicked() {
                            self.splitter.nudge_marker(m_idx, 0.001, total_sec);
                        }
                        if ui.button("+10ms ▶").on_hover_text("+10 Millisekunden").clicked() {
                            self.splitter.nudge_marker(m_idx, 0.01, total_sec);
                        }
                        if ui.button("+100ms ▶").on_hover_text("+100 Millisekunden").clicked() {
                            self.splitter.nudge_marker(m_idx, 0.1, total_sec);
                        }

                        // Snap to nearest transient
                        if !transients.is_empty() {
                            let snap_btn = egui::Button::new(RichText::new("⚡ An Transiente einrasten").color(Color32::from_rgb(255, 215, 0)).strong());
                            if ui.add(snap_btn).on_hover_text("Marker samplegenau an die nächstgelegene Transiente (Beat/Einsatz) springen lassen").clicked() {
                                let mut best_dist = f64::MAX;
                                let mut best_t = m_time;
                                for t in &transients {
                                    let dist = (t.time_sec - m_time).abs();
                                    if dist < best_dist {
                                        best_dist = dist;
                                        best_t = t.time_sec;
                                    }
                                }
                                if best_dist < 2.0 {
                                    self.splitter.push_undo();
                                    self.splitter.markers[m_idx] = best_t;
                                    self.splitter.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
                                }
                            }
                        }

                        let del_btn = egui::Button::new(RichText::new("🗑 Löschen").color(LED_RED));
                        if ui.add(del_btn).clicked() {
                            self.pending_delete_marker = Some((m_idx, m_time, m_title));
                        }
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
                            self.splitter.remove_marker_at_index(idx);
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
            self.splitter.custom_titles.push(format!("Track {:02}", self.splitter.custom_titles.len() + 1));
        }

        render_hardware_rack_card(ui, &self.language.card_tracklist(tracks.len()), |ui| {
            let total_card_w = ui.available_width();
            let fixed_cols_w = 44.0 + 115.0 + 115.0 + 60.0 + 260.0 + 50.0;
            let title_w = (total_card_w - fixed_cols_w).max(280.0);
            let table_total_w = fixed_cols_w + title_w;

            // Scrollable Channel Lanes with synchronized header
            egui::ScrollArea::both()
                .max_height(300.0)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    ui.set_min_width(table_total_w);

                    // A) Table Header Bar
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(44.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new("#").monospace().strong().color(TEXT_MUTED));
                        });
                        ui.allocate_ui_with_layout(Vec2::new(title_w, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(self.language.col_title()).monospace().strong().color(TEXT_MUTED));
                        });
                        ui.allocate_ui_with_layout(Vec2::new(115.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(self.language.col_start()).monospace().strong().color(TEXT_MUTED));
                        });
                        ui.allocate_ui_with_layout(Vec2::new(115.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(self.language.col_end()).monospace().strong().color(TEXT_MUTED));
                        });
                        ui.allocate_ui_with_layout(Vec2::new(60.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(self.language.col_duration()).monospace().strong().color(TEXT_MUTED));
                        });
                        ui.allocate_ui_with_layout(Vec2::new(260.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(RichText::new(self.language.col_actions()).monospace().strong().color(TEXT_MUTED));
                        });
                    });

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    for (i, track) in tracks.iter().enumerate() {
                        let is_selected_seg = self.selected_segment_idx == Some(track.segment_index);
                        let is_this_track_playing = self.player.is_playing()
                            && cur_sec >= track.start_seconds - 0.05
                            && cur_sec < track.end_seconds;

                        let row_bg = if is_this_track_playing {
                            Color32::from_rgba_unmultiplied(37, 99, 235, 45)
                        } else if is_selected_seg {
                            Color32::from_rgba_unmultiplied(255, 215, 0, 25)
                        } else if i % 2 == 0 {
                            Color32::from_rgb(26, 28, 34)
                        } else {
                            Color32::from_rgb(31, 33, 40)
                        };

                        egui::Frame::none()
                            .fill(row_bg)
                            .stroke(Stroke::new(1.0, if is_this_track_playing { Color32::from_rgb(96, 165, 250) } else if is_selected_seg { Color32::from_rgb(200, 160, 40) } else { Color32::TRANSPARENT }))
                            .rounding(4.0)
                            .inner_margin(Vec2::new(6.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 1. Number or Pause
                                    ui.allocate_ui_with_layout(Vec2::new(44.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        if track.is_pause {
                                            ui.label(RichText::new("⏸").color(LED_AMBER).strong());
                                        } else {
                                            ui.label(RichText::new(format!("{:02}", track.index)).strong());
                                        }
                                    });

                                    // 2. Title Edit Field (Dynamic Width)
                                    ui.allocate_ui_with_layout(Vec2::new(title_w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        if track.is_pause {
                                            ui.colored_label(LED_AMBER, self.language.badge_pause());
                                        } else {
                                            let title_buf = &mut self.splitter.custom_titles[i];
                                            let edit_resp = ui.add_sized(
                                                [title_w - 10.0, 24.0],
                                                egui::TextEdit::singleline(title_buf)
                                                    .id_source(egui::Id::new("track_title_field").with(i))
                                                    .hint_text(format!("Track {:02}", track.index))
                                            );

                                            if edit_resp.lost_focus()
                                                && (ui.input(|inp| inp.key_pressed(egui::Key::Enter)) || ui.input(|inp| inp.key_pressed(egui::Key::Tab)))
                                            {
                                                let next_idx = (i + 1) % tracks.len();
                                                ui.ctx().memory_mut(|mem| mem.request_focus(egui::Id::new("track_title_field").with(next_idx)));
                                            }
                                        }
                                    });

                                    // 3. Start time + nudge
                                    ui.allocate_ui_with_layout(Vec2::new(115.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let s_m = (track.start_seconds / 60.0).floor() as u64;
                                        let s_s = (track.start_seconds % 60.0).floor() as u64;
                                        ui.label(RichText::new(format!("{:02}:{:02}", s_m, s_s)).monospace());

                                        if i > 0 {
                                            let m_idx = i - 1;
                                            if ui.small_button("◀").on_hover_text(format!("{} (Shift: 1.0s)", self.language.label_nudge_start())).clicked() {
                                                let delta = if ui.input(|inp| inp.modifiers.shift) { 1.0 } else { 0.1 };
                                                self.splitter.nudge_marker(m_idx, -delta, total_sec);
                                            }
                                            if ui.small_button("▶").on_hover_text(format!("{} (Shift: 1.0s)", self.language.label_nudge_start())).clicked() {
                                                let delta = if ui.input(|inp| inp.modifiers.shift) { 1.0 } else { 0.1 };
                                                self.splitter.nudge_marker(m_idx, delta, total_sec);
                                            }
                                        }
                                    });

                                    // 4. End time + nudge
                                    ui.allocate_ui_with_layout(Vec2::new(115.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let e_m = (track.end_seconds / 60.0).floor() as u64;
                                        let e_s = (track.end_seconds % 60.0).floor() as u64;
                                        ui.label(RichText::new(format!("{:02}:{:02}", e_m, e_s)).monospace());

                                        if i < self.splitter.markers.len() {
                                            let m_idx = i;
                                            if ui.small_button("◀").on_hover_text(format!("{} (Shift: 1.0s)", self.language.label_nudge_end())).clicked() {
                                                let delta = if ui.input(|inp| inp.modifiers.shift) { 1.0 } else { 0.1 };
                                                self.splitter.nudge_marker(m_idx, -delta, total_sec);
                                            }
                                            if ui.small_button("▶").on_hover_text(format!("{} (Shift: 1.0s)", self.language.label_nudge_end())).clicked() {
                                                let delta = if ui.input(|inp| inp.modifiers.shift) { 1.0 } else { 0.1 };
                                                self.splitter.nudge_marker(m_idx, delta, total_sec);
                                            }
                                        }
                                    });

                                    // 5. Duration
                                    ui.allocate_ui_with_layout(Vec2::new(60.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(track.duration_formatted()).monospace());
                                    });

                                    // 6. Actions (Play, Toggle Pause/Track, Scope, Delete)
                                    ui.allocate_ui_with_layout(Vec2::new(260.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let play_btn_text = if is_this_track_playing { "Pause" } else { "Play" };
                                        let play_btn_bg = if is_this_track_playing {
                                            Color32::from_rgb(217, 83, 30)
                                        } else {
                                            Color32::from_rgb(37, 99, 235)
                                        };

                                        let track_btn = egui::Button::new(
                                            RichText::new(play_btn_text).color(Color32::WHITE).small().strong(),
                                        )
                                        .fill(play_btn_bg)
                                        .rounding(3.0);

                                        let tip = if track.is_pause {
                                            "Spielt diesen Pausen-Abschnitt ab"
                                        } else {
                                            "Spielt nur diesen Track ab"
                                        };

                                        if ui.add(track_btn).on_hover_text(tip).clicked() {
                                            if is_this_track_playing {
                                                self.player.pause();
                                            } else {
                                                self.player.play_range(track.start_seconds, track.end_seconds);
                                            }
                                        }

                                        // Scope / Lupe button
                                        let sc_btn = egui::Button::new(
                                            RichText::new(self.language.btn_show_in_scope()).small().color(Color32::from_rgb(255, 215, 0)),
                                        ).rounding(3.0);
                                        if ui.add(sc_btn).on_hover_text("Diesen Track in der Detail-Lupenansicht mit Transienten öffnen").clicked() {
                                            self.selected_region = Some((track.start_seconds, track.end_seconds));
                                            self.selected_segment_idx = Some(track.segment_index);
                                            self.magnifier_scope_active = true;
                                        }

                                        // Toggle Pause / Weglassen button
                                        let p_toggle_text = if track.is_pause {
                                            self.language.btn_make_track()
                                        } else {
                                            self.language.btn_make_pause()
                                        };
                                        let p_btn = egui::Button::new(
                                            RichText::new(p_toggle_text).small().color(if track.is_pause { LED_GREEN } else { TEXT_SECONDARY }),
                                        ).rounding(3.0);
                                        if ui.add(p_btn).clicked() {
                                            self.splitter.toggle_pause(track.segment_index);
                                        }

                                        if i > 0 && i - 1 < self.splitter.markers.len() {
                                            let del_idx = i - 1;
                                            let del_sec = self.splitter.markers[del_idx];
                                            let del_title = self.splitter.custom_titles.get(i).cloned().unwrap_or_else(|| format!("Track {:02}", i + 1));
                                            if ui.small_button(self.language.btn_delete()).clicked() {
                                                self.pending_delete_marker = Some((del_idx, del_sec, del_title));
                                            }
                                        }
                                    });
                                });
                            });
                        ui.add_space(2.0);
                    }
                });

            ui.add_space(4.0);
            ui.label(RichText::new(self.language.tip_nudge_keys()).color(TEXT_MUTED).size(11.0));
        });

        ui.add_space(10.0);

        // 5. Export Drawer
        render_hardware_rack_card(ui, self.language.card_export(), |ui| {
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

            let active_count = tracks.iter().filter(|t| !t.is_pause).count();
            let export_enabled = has_audio && active_count > 0;
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
                let src_file = self.current_audio_path.clone().filter(|p| p.exists())
                    .or_else(|| {
                        let p = self.current_recording_path();
                        if p.exists() { Some(p) } else { None }
                    })
                    .unwrap_or_else(|| self.temp_record_path());
                let target_dir = self.effective_export_directory();
                let _ = std::fs::create_dir_all(&target_dir);

                match export_tracks(&src_file, &target_dir, &tracks, self.normalization_mode.clone()) {
                    Ok(files) => {
                        let success_msg = if self.language == Language::De {
                            format!("[OK] Erfolgreich {} Tracks nach {:?} exportiert!", files.len(), target_dir)
                        } else {
                            format!("[OK] Successfully exported {} tracks to {:?}!", files.len(), target_dir)
                        };
                        self.export_status_message = Some((true, success_msg));
                    }
                    Err(e) => {
                        let err_msg = if self.language == Language::De {
                            format!("[FEHLER] Export-Fehler: {}", e)
                        } else {
                            format!("[ERROR] Export error: {}", e)
                        };
                        self.export_status_message = Some((false, err_msg));
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
    }
}

fn render_hardware_rack_card<F: FnOnce(&mut Ui)>(ui: &mut Ui, rack_title: &str, content: F) {
    egui::Frame::none()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_DARK))
        .rounding(6.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.label(RichText::new(rack_title).size(10.5).monospace().strong().color(TEXT_MUTED));
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

fn generate_stereo_waveform_from_samples(
    samples_l: &[f32],
    samples_r: &[f32],
    total: usize,
) -> Vec<StereoWaveformPoint> {
    if total == 0 {
        return Vec::new();
    }
    let target_points = 4800;
    let chunk_size = (total / target_points).max(1);

    let mut points = Vec::new();
    let l_chunks = samples_l.chunks(chunk_size);
    let r_chunks = samples_r.chunks(chunk_size);

    for (chunk_l, chunk_r) in l_chunks.zip(r_chunks) {
        let mut min_l = 0.0_f32;
        let mut max_l = 0.0_f32;
        for &s in chunk_l {
            if s < min_l { min_l = s; }
            if s > max_l { max_l = s; }
        }

        let mut min_r = 0.0_f32;
        let mut max_r = 0.0_f32;
        for &s in chunk_r {
            if s < min_r { min_r = s; }
            if s > max_r { max_r = s; }
        }

        points.push(StereoWaveformPoint {
            min_l,
            max_l,
            min_r,
            max_r,
        });
    }
    points
}

fn draw_vector_cassette(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 3.0, Color32::from_rgb(26, 28, 34));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.2, LED_AMBER));
    let y = rect.center().y;
    painter.circle_stroke(Pos2::new(rect.min.x + 8.0, y), 3.5, Stroke::new(1.2, LED_AMBER));
    painter.circle_stroke(Pos2::new(rect.max.x - 8.0, y), 3.5, Stroke::new(1.2, LED_AMBER));
    let window_rect = Rect::from_center_size(rect.center(), Vec2::new(12.0, 7.0));
    painter.rect_stroke(window_rect, 1.0, Stroke::new(1.0, Color32::from_rgb(70, 74, 86)));
}

#[cfg(test)]
mod tests {
    use super::*;

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
            waveform_cache: Vec::new(),
            selected_marker_idx: None,
            selected_segment_idx: None,
            waveform_zoom: 1.0,
            waveform_vertical_zoom: 1.0,
            waveform_height: 280.0,
            magnifier_scope_active: false,
            selected_region: None,
            transient_sensitivity: 1.0,
            show_transients: true,
            scope_zoom: 1.0,
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

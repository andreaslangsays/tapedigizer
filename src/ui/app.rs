use std::path::PathBuf;
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
    waveform_zoom: f32,
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

        let app = Self {
            language: lang,
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
            waveform_zoom: 1.0,
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
        };
        cfg.save();
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
        let temp_path = self.temp_record_path();
        if temp_path.exists() {
            match analyze_wav_file(&temp_path) {
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

            if let Err(e) = self.player.load_file(&temp_path) {
                eprintln!("Fehler beim Laden im Player: {}", e);
            }

            self.waveform_cache = self.recorder.get_waveform_data();
            if self.waveform_cache.is_empty() {
                self.waveform_cache = generate_stereo_waveform_from_samples(
                    &self.player.get_samples_l(),
                    &self.player.get_samples_r(),
                    self.player.total_samples(),
                );
            }

            self.splitter.clear_markers();
        } else {
            self.ui_error_message = Some("Aufnahmedatei wurde nicht auf der Festplatte gefunden.".into());
            self.active_tab = ActiveTab::Diagnosis;
        }
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

        // Global Shortcuts in Tab 3 (Split & Export)
        if self.active_tab == ActiveTab::SplitAndExport && !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.player.toggle_play();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::M)) {
                let cur_sec = self.player.current_seconds();
                let total_sec = self.player.total_seconds();
                self.splitter.add_marker(cur_sec, total_sec);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::H)) {
                self.hover_preview_enabled = !self.hover_preview_enabled;
                if !self.hover_preview_enabled && self.hover_is_auditioning {
                    self.player.pause();
                    self.hover_is_auditioning = false;
                    self.hover_audition_time = None;
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::C)) {
                self.active_edit_tool = if self.active_edit_tool == EditTool::Scissor {
                    EditTool::Pointer
                } else {
                    EditTool::Scissor
                };
            }
            if ctx.input(|i| i.key_pressed(egui::Key::V) || i.key_pressed(egui::Key::Escape)) {
                self.active_edit_tool = EditTool::Pointer;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                if let Some(idx) = self.selected_marker_idx.take() {
                    self.splitter.remove_marker_at_index(idx);
                }
            }
        }

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
                    ui.label(RichText::new("v0.2.1").size(9.0).monospace().color(Color32::from_rgb(90, 95, 110)));

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

                            let target_path = self.temp_record_path();
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

        // Stereo Waveform Rack Card
        render_hardware_rack_card(ui, self.language.card_arrange(), |ui| {
            // Header Bar: Tool Modes, Hover Audition, Timecode
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

                // Timecode Display
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cur_m = (cur_sec / 60.0).floor() as u64;
                    let cur_s = (cur_sec % 60.0).floor() as u64;
                    let cur_ms = ((cur_sec % 1.0) * 10.0).floor() as u64;

                    let tot_m = (total_sec / 60.0).floor() as u64;
                    let tot_s = (total_sec % 60.0).floor() as u64;

                    ui.label(RichText::new(format!("{:02}:{:02}.{} / {:02}:{:02}", cur_m, cur_s, cur_ms, tot_m, tot_s)).size(15.0).monospace().strong().color(Color32::WHITE));
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
                &mut self.selected_marker_idx,
                &mut self.waveform_zoom,
                &mut self.waveform_scroll,
                self.active_edit_tool,
                self.hover_is_auditioning,
                self.hover_audition_time,
            );

            if let Some(target_sec) = interaction.seek_to_seconds {
                self.player.seek_seconds(target_sec);
            }
            if let Some(new_marker_sec) = interaction.add_marker_at_seconds {
                self.splitter.add_marker(new_marker_sec, total_sec);
            }
            if let Some((idx, new_time)) = interaction.dragged_marker {
                if idx < self.splitter.markers.len() {
                    self.splitter.markers[idx] = new_time.clamp(0.5, total_sec - 0.5);
                    self.splitter.markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
                }
            }
            if let Some(del_idx) = interaction.delete_marker_idx {
                self.splitter.remove_marker_at_index(del_idx);
            }

            // Hover Audition Engine
            if self.hover_preview_enabled {
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

            // Transport & Toolbar
            ui.horizontal(|ui| {
                let is_playing = self.player.is_playing();
                let (play_text, play_col, play_stroke) = if is_playing {
                    (self.language.btn_pause(), Color32::from_rgb(217, 83, 30), Color32::from_rgb(251, 146, 60))
                } else {
                    (self.language.btn_play(), Color32::from_rgb(37, 99, 235), Color32::from_rgb(96, 165, 250))
                };

                let play_btn = egui::Button::new(RichText::new(play_text).strong().size(13.0).color(Color32::WHITE))
                    .fill(play_col)
                    .stroke(Stroke::new(1.0, play_stroke))
                    .rounding(5.0)
                    .min_size(Vec2::new(130.0, 32.0));

                if ui.add(play_btn).clicked() {
                    self.player.toggle_play();
                }

                if ui.button(self.language.btn_stop()).clicked() {
                    self.player.stop();
                }

                ui.add_space(14.0);

                let add_m_btn = egui::Button::new(RichText::new(self.language.btn_add_marker()).color(LED_AMBER).strong())
                    .rounding(5.0);
                if ui.add(add_m_btn).clicked() {
                    self.splitter.add_marker(cur_sec, total_sec);
                }

                if ui.button(self.language.btn_auto_detect_pauses()).clicked() {
                    let samples_l = self.player.get_samples_l();
                    let samples_r = self.player.get_samples_r();
                    self.splitter.detect_silence_pauses(&samples_l, &samples_r, self.player.total_samples() as u32, -45.0, 1.8);
                }

                if ui.button(self.language.btn_clear_markers()).clicked() {
                    self.splitter.clear_markers();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("1x Reset").on_hover_text("1x").clicked() {
                        self.waveform_zoom = 1.0;
                        self.waveform_scroll = 0.0;
                    }
                    ui.add(egui::Slider::new(&mut self.waveform_zoom, 1.0..=25.0).text("Zoom").suffix("x"));
                });
            });
        });

        ui.add_space(12.0);

        // Tracklist Table
        let tracks = self.splitter.get_tracks(total_sec);

        render_hardware_rack_card(ui, &self.language.card_tracklist(tracks.len()), |ui| {
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                egui::Grid::new("tracks_table").striped(true).spacing(Vec2::new(16.0, 6.0)).show(ui, |ui| {
                    ui.label(RichText::new("#").strong());
                    ui.label(RichText::new(self.language.col_title()).strong());
                    ui.label(RichText::new(self.language.col_start()).strong());
                    ui.label(RichText::new(self.language.col_end()).strong());
                    ui.label(RichText::new(self.language.col_duration()).strong());
                    ui.label(RichText::new(self.language.col_actions()).strong());
                    ui.end_row();

                    while self.splitter.custom_titles.len() < tracks.len() {
                        self.splitter.custom_titles.push(format!("Track {:02}", self.splitter.custom_titles.len() + 1));
                    }

                    for (i, track) in tracks.iter().enumerate() {
                        ui.label(format!("{:02}", track.index));

                        let title_buf = &mut self.splitter.custom_titles[i];
                        ui.text_edit_singleline(title_buf);

                        let s_m = (track.start_seconds / 60.0).floor() as u64;
                        let s_s = (track.start_seconds % 60.0).floor() as u64;
                        ui.label(format!("{:02}:{:02}", s_m, s_s));

                        let e_m = (track.end_seconds / 60.0).floor() as u64;
                        let e_s = (track.end_seconds % 60.0).floor() as u64;
                        ui.label(format!("{:02}:{:02}", e_m, e_s));

                        ui.label(track.duration_formatted());

                        ui.horizontal(|ui| {
                            if ui.small_button("Play").clicked() {
                                self.player.seek_seconds(track.start_seconds);
                                self.player.play();
                            }
                            if i > 0 && i - 1 < self.splitter.markers.len() {
                                if ui.small_button(self.language.btn_delete()).clicked() {
                                    self.splitter.remove_marker_at_index(i - 1);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        });

        ui.add_space(12.0);

        // Export Drawer
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

            ui.add_space(14.0);

            let export_btn = egui::Button::new(
                RichText::new(self.language.btn_export_tracks(tracks.len()))
                    .size(15.0)
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(Color32::from_rgb(37, 99, 235))
            .stroke(Stroke::new(1.0, Color32::from_rgb(96, 165, 250)))
            .rounding(6.0)
            .min_size(Vec2::new(380.0, 42.0));

            if ui.add(export_btn).clicked() {
                let src_file = self.temp_record_path();
                match export_tracks(&src_file, &self.export_directory, &tracks, self.normalization_mode.clone()) {
                    Ok(files) => {
                        let success_msg = if self.language == Language::De {
                            format!("[OK] Erfolgreich {} Tracks nach {:?} exportiert!", files.len(), self.export_directory)
                        } else {
                            format!("[OK] Successfully exported {} tracks to {:?}!", files.len(), self.export_directory)
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
                    if is_success && ui.button(self.language.btn_show_in_finder()).clicked() {
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&self.export_directory).spawn();
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
    let target_points = 900;
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

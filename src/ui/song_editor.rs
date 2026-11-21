//! A focused single-track workspace under Editing: local transport, nondestructive processing and no neighboring songs.
use super::theme::{
    bevel_button, bevel_button_sized, draw_recessed_well, draw_vertical_stereo_meter,
    hardware_fader, module_card, palette, panel_title, rack_columns, rotary_colored,
    rotary_colored_sized, section_title, segmented_control, surface_bay, time_display, BevelStyle,
    ACCENT, FX_ACCENT, HEIGHT_CONTROL, LED_AMBER, LED_RED, RADIUS_WELL,
};
use super::waveform_widget::{render_stereo_waveform, EditTool};
use crate::audio::denoiser::{AuditionMode, NoiseProfile};
use crate::audio::meter::{linear_to_db, MeterLevels};
use crate::audio::player::AudioPlayer;
use crate::audio::processing::{
    ChannelSettings, DynamicsSettings, LimiterSettings, Routing, SongExportMode, SongSettings,
};
use crate::audio::recorder::StereoWaveformPoint;
use crate::i18n::Language;
use crate::project::SongEditorView;
use crate::splitter::{MarkerSnapshot, TrackInfo, TrackSplitter};
use egui::Ui;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SongEditorAction {
    BackToCut,
    Export(usize),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Module {
    Channels,
    Restoration,
    Equalizer,
    Dynamics,
    #[default]
    Mix,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DynamicsUnit {
    #[default]
    Compressor,
    Expander,
    Limiter,
}

type ClipKey = (usize, usize, usize, usize, u32);

pub struct SongEditor {
    pub view: SongEditorView,
    player: AudioPlayer,
    clip: Option<ClipKey>,
    waveform: Vec<StereoWaveformPoint>,
    mono_waveform: Vec<StereoWaveformPoint>,
    mono_wave_key: Option<(Routing, u32, u32)>,
    pub selection: Option<(f64, f64)>,
    original: bool,
    solo: Option<usize>,
    mono_audition: bool,
    source_view: bool,
    select_tool: bool,
    channel: usize,
    module: Module,
    dynamics_unit: DynamicsUnit,
    active: bool,
    error: Option<String>,
    edit_transaction: Option<MarkerSnapshot>,
}

impl Default for SongEditor {
    fn default() -> Self {
        Self {
            view: SongEditorView::default(),
            player: AudioPlayer::new(),
            clip: None,
            waveform: Vec::new(),
            mono_waveform: Vec::new(),
            mono_wave_key: None,
            selection: None,
            original: false,
            solo: None,
            mono_audition: false,
            source_view: false,
            select_tool: false,
            channel: 0,
            module: Module::Mix,
            dynamics_unit: DynamicsUnit::Compressor,
            active: false,
            error: None,
            edit_transaction: None,
        }
    }
}

impl SongEditor {
    #[cfg(test)]
    pub(crate) fn select_layout_test_module(&mut self, index: usize) {
        self.module = [
            Module::Mix,
            Module::Channels,
            Module::Restoration,
            Module::Equalizer,
            Module::Dynamics,
            Module::Dynamics,
        ][index.min(5)];
        self.dynamics_unit = match index {
            4 => DynamicsUnit::Expander,
            5 => DynamicsUnit::Limiter,
            _ => DynamicsUnit::Compressor,
        };
    }

    #[cfg(test)]
    pub(crate) fn layout_test_module(&self) -> u8 {
        self.module as u8
    }

    pub fn suspend(&mut self) {
        self.player.pause();
        self.active = false;
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    pub fn seek_by(&self, seconds: f64) {
        self.player.seek_seconds(
            (self.player.current_seconds() + seconds).clamp(0.0, self.player.total_seconds()),
        );
    }

    pub fn use_range_tool(&mut self, enabled: bool) {
        self.select_tool = enabled;
    }

    pub fn finish_edit(&mut self, splitter: &mut TrackSplitter) {
        if let Some(snapshot) = self.edit_transaction.take() {
            splitter.commit_edit(snapshot);
        }
    }

    pub fn toggle_play(&mut self) {
        if !self.active {
            return;
        }
        if self.player.is_playing() {
            self.player.pause();
            return;
        }
        if self.player.total_samples() == 0 {
            return;
        }
        if let Err(error) = self.player.ensure_output() {
            self.error = Some(error);
            return;
        }
        self.player.set_looping(self.view.loop_playback);
        let (start, end) = self.selection.unwrap_or((0.0, self.player.total_seconds()));
        self.player.toggle_range(start, end);
    }

    fn prepare(&mut self, left: Arc<Vec<f32>>, right: Arc<Vec<f32>>, rate: u32, track: &TrackInfo) {
        let start = (track.start_seconds * f64::from(rate)).round() as usize;
        let end = (track.end_seconds * f64::from(rate)).round() as usize;
        let key = (
            Arc::as_ptr(&left) as usize,
            Arc::as_ptr(&right) as usize,
            start,
            end,
            rate,
        );
        if self.clip == Some(key) {
            return;
        }
        if self.view.selected_segment != Some(track.segment_index) {
            self.view.zoom = 1.0;
            self.view.vertical_zoom = 1.0;
            self.view.scroll = 0.0;
        }
        self.view.selected_segment = Some(track.segment_index);
        self.selection = None;
        self.original = false;
        self.solo = None;
        self.mono_audition = false;
        self.error = None;
        self.waveform.clear();
        self.mono_wave_key = None;
        self.mono_waveform.clear();
        if let Err(error) =
            self.player
                .load_song_clip(left.clone(), right.clone(), rate, start, end)
        {
            self.error = Some(error);
            self.clip = None;
            return;
        }
        let frames = end - start;
        let bins = frames.min(65_536);
        for bin in 0..bins {
            let a = start + bin * frames / bins;
            let b = start + (bin + 1) * frames / bins;
            let mut point = StereoWaveformPoint::default();
            for (&l, &r) in left[a..b].iter().zip(&right[a..b]) {
                point.min_l = point.min_l.min(l);
                point.max_l = point.max_l.max(l);
                point.min_r = point.min_r.min(r);
                point.max_r = point.max_r.max(r);
            }
            self.waveform.push(point);
        }
        self.clip = Some(key);
    }

    pub fn learn_noise(
        &mut self,
        settings: &mut SongSettings,
        left: &[f32],
        right: &[f32],
        rate: u32,
        track: &TrackInfo,
    ) {
        let Some((start, end)) = self.selection else {
            self.error = Some("Bitte zuerst einen Abschnitt mit reinem Rauschen auswählen. / Select a noise-only range first.".into());
            return;
        };
        let offset = (track.start_seconds * f64::from(rate)).round() as usize;
        let first = offset + (start * f64::from(rate)).round() as usize;
        let last = (offset + (end * f64::from(rate)).round() as usize)
            .min(left.len())
            .min(right.len());
        if first >= last || last - first < 256 {
            self.error = Some("Rauschbereich zu kurz / Noise range too short".into());
            return;
        }
        match NoiseProfile::learn_from_samples(
            &left[first..last],
            &right[first..last],
            rate,
            track.title.clone(),
            "Song",
        ) {
            Ok(profile) => {
                settings.noise_profile = Some(profile);
                for channel in &mut settings.channels {
                    channel.denoiser.enabled = true;
                    channel.denoiser.audition_mode = AuditionMode::Wet;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        left: Arc<Vec<f32>>,
        right: Arc<Vec<f32>>,
        rate: u32,
        tracks: &[TrackInfo],
        selected: &mut Option<usize>,
        splitter: &mut TrackSplitter,
        legacy: &SongSettings,
        legacy_highpass: bool,
        presets: &[NoiseProfile],
        lang: Language,
    ) -> Option<SongEditorAction> {
        let t = |de, en| if lang == Language::De { de } else { en };
        let available = ui.available_height();
        let requested = if self.active {
            self.view.selected_segment
        } else {
            selected.or(self.view.selected_segment)
        };
        let mut choice = requested
            .and_then(|idx| {
                tracks
                    .iter()
                    .find(|track| track.segment_index == idx && !track.is_pause)
            })
            .or_else(|| tracks.iter().find(|track| !track.is_pause))
            .map(|track| track.segment_index);
        if choice.is_none() {
            self.suspend();
            self.clip = None;
            self.player.reset();
            let mut back = false;
            surface_bay(ui, |ui| {
                ui.heading(t("Kein Track ausgewählt", "No track selected"));
                ui.label(t(
                    "Lege im Schnittbereich mindestens einen Track an.",
                    "Create at least one track in the cutting workspace.",
                ));
                ui.add_space(8.0);
                if bevel_button(ui, t("← Zurück zum Schnitt", "← Back to editing"), BevelStyle::Neutral).clicked() {
                    back = true;
                }
            });
            return back.then_some(SongEditorAction::BackToCut);
        }
        let mut export_song = false;
        let mut back_to_cut = false;
        let Some(track) =
            choice.and_then(|idx| tracks.iter().find(|track| track.segment_index == idx))
        else {
            return None;
        };
        self.prepare(left.clone(), right.clone(), rate, track);
        self.active = true;
        *selected = Some(track.segment_index);
        let independent = splitter.song_settings.contains_key(&track.segment_index);
        let mut settings = splitter
            .song_settings
            .get(&track.segment_index)
            .cloned()
            .unwrap_or_else(|| {
                let mut inherited = legacy.clone();
                if !track.is_filtered {
                    inherited.legacy_highpass_20hz = legacy_highpass;
                    for channel in &mut inherited.channels {
                        channel.denoiser.enabled = false;
                        channel.denoiser.dc_filter_enabled = false;
                    }
                }
                inherited
            });
        self.update_monitor(&settings);
        let before = settings.clone();
        let mut adopt = false;
        let (l, r, clip) = self.player.output_peaks();
        let levels = MeterLevels {
            peak_l_db: linear_to_db(l),
            peak_r_db: linear_to_db(r),
            peak_hold_l_db: linear_to_db(l),
            peak_hold_r_db: linear_to_db(r),
            is_clipping: clip,
            ..Default::default()
        };
        // Kompakte Kopf-Bay: Track-Fokus + Transport + Ausgangspegel
        surface_bay(ui, |ui| {
            ui.horizontal(|ui| {
                if bevel_button(
                    ui,
                    t("← Schnitt", "← Editing"),
                    BevelStyle::Neutral,
                )
                .clicked()
                {
                    back_to_cut = true;
                }
                panel_title(ui, t("Track-Fokus", "Track focus"));
                let title = format!(
                    "{:02} · {}   ({})",
                    track.index,
                    track.title,
                    track.duration_formatted()
                );
                let combo_w = (ui.available_width() - 400.0).clamp(130.0, 400.0);
                egui::ComboBox::from_id_salt("song_studio_selector")
                    .selected_text(title)
                    .width(combo_w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for track in tracks.iter().filter(|track| !track.is_pause) {
                            ui.selectable_value(
                                &mut choice,
                                Some(track.segment_index),
                                format!(
                                    "{:02} · {}   {}",
                                    track.index,
                                    track.title,
                                    track.duration_formatted()
                                ),
                            );
                        }
                    });
                segmented_control(
                    ui,
                    "song_ab_monitor",
                    &mut self.original,
                    &[
                        (false, t("Bearbeitet", "Processed"), ""),
                        (true, t("Original", "Original"), ""),
                    ],
                    HEIGHT_CONTROL,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    export_song = bevel_button(
                        ui,
                        t("Track exportieren …", "Export track …"),
                        BevelStyle::Primary,
                    )
                    .clicked();
                });
            });
            ui.add_space(5.0);
            rack_columns(ui, 2, |cols| {
                let ui = &mut cols[0];
                ui.horizontal_wrapped(|ui| {
                    let play = if self.player.is_playing() {
                        t("Pause", "Pause")
                    } else {
                        t("Wiedergabe", "Play")
                    };
                    if bevel_button_sized(
                        ui,
                        play,
                        84.0,
                        if self.player.is_playing() {
                            BevelStyle::Primary
                        } else {
                            BevelStyle::Neutral
                        },
                    )
                    .on_hover_text(t("Leertaste · nur dieser Song", "Space · this song only"))
                    .clicked()
                    {
                        self.toggle_play();
                    }
                    if bevel_button_sized(ui, t("Stopp", "Stop"), 62.0, BevelStyle::Neutral)
                        .clicked()
                    {
                        self.player.stop();
                    }
                    time_display(
                        ui,
                        format!(
                            "{} / {}",
                            timecode(self.player.current_seconds()),
                            timecode(self.player.total_seconds())
                        ),
                    );
                    ui.checkbox(&mut self.view.loop_playback, t("Schleife", "Loop"));
                    ui.checkbox(&mut self.mono_audition, t("Mono", "Mono"));
                    if self.original || self.solo.is_some() || self.mono_audition {
                        ui.colored_label(LED_AMBER, t("Monitor", "Monitor"));
                    }
                });
                let meter_label = t("Ausgang · dBFS", "Output · dBFS");
                if super::meter_widget::render_stereo_meter(
                    &mut cols[1],
                    &levels,
                    meter_label,
                    t("CLIP zurücksetzen", "Reset clip"),
                    t("Übersteuerung erkannt", "Clipping detected"),
                ) {
                    self.player.reset_output_clip();
                }
            });
        });
        if back_to_cut {
            self.suspend();
            return Some(SongEditorAction::BackToCut);
        }
        if choice != Some(track.segment_index) {
            *selected = choice;
            self.view.selected_segment = choice;
        }
        self.player.set_looping(self.view.loop_playback);
        if let Some(error) = self.player.take_stream_error() {
            self.error = Some(error);
        }
        if let Some(error) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(LED_RED, error);
                if bevel_button(ui, "×", BevelStyle::Compact).clicked() {
                    self.error = None;
                }
            });
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            section_title(ui, t("Song-Wellenform", "Song waveform"), FX_ACCENT);
            ui.colored_label(
                ACCENT,
                if self.source_view || self.original {
                    t("Quelle · L/R", "Source · L/R")
                } else {
                    routing_label(settings.routing, lang)
                },
            );
            segmented_control(
                ui,
                "song_wave_tool",
                &mut self.select_tool,
                &[
                    (false, t("Zeiger", "Pointer"), ""),
                    (true, t("Bereich wählen", "Select range"), ""),
                ],
                HEIGHT_CONTROL,
            );
            ui.spacing_mut().slider_width = 68.0;
            ui.add(
                egui::Slider::new(&mut self.view.zoom, 1.0..=30.0)
                    .logarithmic(true)
                    .prefix("Zoom ×")
                    .max_decimals(1),
            );
            ui.add(
                egui::Slider::new(&mut self.view.vertical_zoom, 1.0..=16.0)
                    .logarithmic(true)
                    .prefix(t("Pegel ×", "Amp ×"))
                    .max_decimals(1),
            );
            ui.checkbox(&mut self.source_view, t("Quellkanäle L/R", "Source L/R"));
            if let Some((a, b)) = self.selection {
                ui.monospace(format!("{} – {}", timecode(a), timecode(b)));
                if bevel_button(
                    ui,
                    t("Auswahl aufheben", "Clear selection"),
                    BevelStyle::Compact,
                )
                .clicked()
                {
                    self.selection = None;
                }
            }
        });
        let mono = settings.routing != Routing::Stereo && !self.source_view && !self.original;
        let wave = if mono {
            let key = (
                settings.routing,
                settings.mix_left.to_bits(),
                settings.mix_right.to_bits(),
            );
            if self.mono_wave_key != Some(key) {
                let start = (track.start_seconds * f64::from(rate)).round() as usize;
                let end = (track.end_seconds * f64::from(rate)).round() as usize;
                self.mono_waveform =
                    mono_waveform(&left[start..end], &right[start..end], &settings);
                self.mono_wave_key = Some(key);
            }
            &self.mono_waveform
        } else {
            &self.waveform
        };
        let mut marker = None;
        let interaction = render_stereo_waveform(
            ui,
            wave,
            self.player.total_seconds(),
            self.player.current_seconds(),
            self.player.is_playing(),
            &[],
            &[],
            &[],
            &mut marker,
            self.selection,
            &mut self.view.zoom,
            &mut self.view.vertical_zoom,
            (available * 0.13).clamp(60.0, 124.0),
            &mut self.view.scroll,
            if self.select_tool {
                EditTool::Select
            } else {
                EditTool::Pointer
            },
            false,
            None,
            mono,
        );
        if let Some(time) = interaction.seek_to_seconds {
            self.player.seek_seconds(time);
        }
        if let Some(region) = interaction.new_selected_region {
            self.selection = Some(region);
        }
        ui.add_space(6.0);
        // Processing-Rack: skalierend & ohne Scrollbar (alles direkt im Blick)
        surface_bay(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                segmented_control(
                    ui,
                    "song_processing_signal_flow",
                    &mut self.module,
                    &[
                        (Module::Channels, t("Kanäle", "Channels"), ""),
                        (Module::Restoration, t("Restauration", "Restoration"), ""),
                        (Module::Equalizer, "Equalizer", ""),
                        (Module::Dynamics, t("Dynamik", "Dynamics"), ""),
                        (Module::Mix, t("Mix & Ausgabe", "Mix & export"), ""),
                    ],
                    HEIGHT_CONTROL,
                );
                if !independent {
                    ui.add_space(6.0);
                    if bevel_button(
                        ui,
                        t("Eigenständig bearbeiten", "Independent processing"),
                        BevelStyle::Compact,
                    )
                    .on_hover_text(t(
                        "Bisherige Projekteinstellungen · Änderungen gelten nur für diesen Track",
                        "Inherited project settings · changes affect this track only",
                    ))
                    .clicked()
                    {
                        adopt = true;
                    }
                }
            });
            ui.add_space(4.0);
            let rack_h = ui.available_height();
            let knob_size = if rack_h < 270.0 {
                36.0
            } else if rack_h < 330.0 {
                42.0
            } else {
                50.0
            };
            ui.scope(|ui| {
                        if self.module == Module::Mix {
                            let p = palette();
                            rack_columns(ui, 3, |columns| {
                                module_card(
                                    &mut columns[0],
                                    "01",
                                    t("Kanal & Routing", "Channel & routing"),
                                    t("Summier-Matrix", "Summing matrix"),
                                    p.accent_top,
                                    |ui| {
                                        egui::ComboBox::from_id_salt("song_routing")
                                            .selected_text(routing_label(settings.routing, lang))
                                            .width((ui.available_width() - 8.0).min(240.0))
                                            .truncate()
                                            .show_ui(ui, |ui| {
                                                for routing in [
                                                    Routing::Stereo,
                                                    Routing::LeftMono,
                                                    Routing::RightMono,
                                                    Routing::MonoMix,
                                                ] {
                                                    ui.selectable_value(
                                                        &mut settings.routing,
                                                        routing,
                                                        routing_label(routing, lang),
                                                    );
                                                }
                                            });
                                        ui.add_space(6.0);
                                        if settings.routing == Routing::MonoMix {
                                            ui.horizontal(|ui| {
                                                hardware_fader(ui, t("Links", "Left"), &mut settings.mix_left);
                                                hardware_fader(ui, t("Rechts", "Right"), &mut settings.mix_right);
                                            });
                                            ui.add_space(4.0);
                                            segmented_control(
                                                ui,
                                                "monomix_solo",
                                                &mut self.solo,
                                                &[
                                                    (Some(0), t("L solo", "Solo L"), ""),
                                                    (Some(1), t("R solo", "Solo R"), ""),
                                                    (None, t("Mix", "Mix"), ""),
                                                ],
                                                HEIGHT_CONTROL,
                                            );
                                        } else if settings.routing == Routing::Stereo {
                                            ui.columns(2, |cols| {
                                                rotary_colored(
                                                    &mut cols[0],
                                                    t("Balance", "Balance"),
                                                    &mut settings.balance_db,
                                                    -12.0..=12.0,
                                                    " dB",
                                                    p.accent_top,
                                                );
                                                rotary_colored(
                                                    &mut cols[1],
                                                    t("Breite", "Width"),
                                                    &mut settings.width,
                                                    0.0..=2.0,
                                                    " ×",
                                                    p.teal,
                                                );
                                            });
                                            ui.checkbox(
                                                &mut settings.swap_channels,
                                                t("Links / Rechts tauschen", "Swap left / right"),
                                            );
                                        }
                                    },
                                );

                                module_card(
                                    &mut columns[1],
                                    "02",
                                    t("Ausgangspegel", "Output level"),
                                    "Mastering Bus",
                                    p.teal,
                                    |ui| {
                                        if settings.routing == Routing::Stereo {
                                            let meter_h = (rack_h - 110.0).clamp(80.0, 118.0);
                                            ui.columns(2, |pair| {
                                                rotary_colored_sized(
                                                    &mut pair[0],
                                                    t("Song-Pegel", "Song gain"),
                                                    &mut settings.gain_db,
                                                    -24.0..=12.0,
                                                    " dB",
                                                    p.accent_top,
                                                    knob_size,
                                                );
                                                pair[1].vertical_centered(|ui| {
                                                    if draw_vertical_stereo_meter(
                                                        ui,
                                                        levels.peak_l_db,
                                                        levels.peak_r_db,
                                                        levels.is_clipping,
                                                        meter_h,
                                                    ) {
                                                        self.player.reset_output_clip();
                                                    }
                                                });
                                            });
                                        } else {
                                            ui.columns(2, |cols| {
                                                rotary_colored(
                                                    &mut cols[0],
                                                    t("Song-Pegel", "Song gain"),
                                                    &mut settings.gain_db,
                                                    -24.0..=12.0,
                                                    " dB",
                                                    p.accent_top,
                                                );
                                                rotary_colored(
                                                    &mut cols[1],
                                                    t("Mono-Ausgang", "Mono out"),
                                                    &mut settings.mono_gain_db,
                                                    -24.0..=12.0,
                                                    " dB",
                                                    p.teal,
                                                );
                                            });
                                        }
                                    },
                                );

                                module_card(
                                    &mut columns[2],
                                    "03",
                                    t("Ausgabe & Bypass", "Export & bypass"),
                                    t("Ziel-Exportformat", "Target format"),
                                    p.amber,
                                    |ui| {
                                        egui::ComboBox::from_id_salt("song_export_format")
                                            .selected_text(export_label(settings.export_mode, lang))
                                            .width((ui.available_width() - 8.0).min(240.0))
                                            .truncate()
                                            .show_ui(ui, |ui| {
                                                for format in [
                                                    SongExportMode::AsEdited,
                                                    SongExportMode::Stereo,
                                                    SongExportMode::SeparateMono,
                                                ] {
                                                    ui.selectable_value(
                                                        &mut settings.export_mode,
                                                        format,
                                                        export_label(format, lang),
                                                    );
                                                }
                                            });
                                        ui.add_space(6.0);
                                        ui.checkbox(
                                            &mut settings.bypass,
                                            t("Master-Bypass", "Master bypass"),
                                        )
                                        .on_hover_text(t(
                                            "Gesamte Track-Bearbeitung umgehen",
                                            "Bypass all track processing",
                                        ));
                                        if legacy_highpass || settings.legacy_highpass_20hz {
                                            ui.checkbox(
                                                &mut settings.legacy_highpass_20hz,
                                                t("20-Hz-Filter aus Altprojekt", "Legacy 20 Hz filter"),
                                            );
                                        }
                                        ui.add_space(8.0);
                                        if bevel_button(
                                            ui,
                                            t("Zurücksetzen", "Reset track"),
                                            BevelStyle::Neutral,
                                        )
                                        .clicked()
                                        {
                                            settings = SongSettings::default();
                                            adopt = true;
                                        }
                                    },
                                );
                            });
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                panel_title(ui, t("Kanal", "Channel"));
                                segmented_control(
                                    ui,
                                    "song_channel_select",
                                    &mut self.channel,
                                    &[(0, t("Links", "Left"), ""), (1, t("Rechts", "Right"), "")],
                                    HEIGHT_CONTROL,
                                );
                                ui.checkbox(
                                    &mut settings.channels_linked,
                                    t("Regler L/R koppeln", "Link L/R controls"),
                                );
                                if self.module == Module::Dynamics {
                                    ui.checkbox(
                                        &mut settings.stereo_dynamics_linked,
                                        t("Detektoren koppeln", "Link detectors"),
                                    );
                                }
                                let solo = self.solo == Some(self.channel);
                                if bevel_button(
                                    ui,
                                    t("Kanal solo", "Solo channel"),
                                    if solo {
                                        BevelStyle::Engaged
                                    } else {
                                        BevelStyle::Neutral
                                    },
                                )
                                .clicked()
                                {
                                    self.solo = if solo { None } else { Some(self.channel) };
                                }
                            });
                            let channel_before = settings.channels[self.channel].clone();
                            ui.add_space(4.0);
                            channel_controls(
                                ui,
                                &mut settings.channels[self.channel],
                                self.module,
                                &mut self.dynamics_unit,
                                rack_h,
                                knob_size,
                                lang,
                            );
                            if settings.channels_linked {
                                let edited = settings.channels[self.channel].clone();
                                link_changed_controls(
                                    &channel_before,
                                    &edited,
                                    &mut settings.channels[1 - self.channel],
                                );
                            }
                        }
                        if self.module == Module::Restoration {
                            ui.add_space(3.0);
                            let ch = &settings.channels[self.channel];
                            let spectrum_h = (ui.available_height() - 36.0).clamp(40.0, 128.0);
                            draw_noise_spectrum_curve(
                                ui,
                                settings.noise_profile.as_ref(),
                                ch.denoiser.reduction_db,
                                ch.denoiser.dc_filter_enabled.then_some(ch.denoiser.dc_cutoff_hz),
                                spectrum_h,
                                lang,
                            );
                            ui.add_space(4.0);
                            let p = palette();
                            let profile_sub = settings.noise_profile.as_ref().map_or_else(
                                || t("Kein Profil", "No profile").to_string(),
                                |profile| profile.name.clone(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                let badge_bg = egui::Color32::from_rgba_unmultiplied(
                                    p.accent_top.r(),
                                    p.accent_top.g(),
                                    p.accent_top.b(),
                                    if p.is_dark { 42 } else { 26 },
                                );
                                egui::Frame::none()
                                    .fill(badge_bg)
                                    .rounding(4.0)
                                    .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("NP")
                                                .monospace()
                                                .size(9.5)
                                                .strong()
                                                .color(p.accent_top),
                                        );
                                    });
                                ui.label(
                                    egui::RichText::new(profile_sub)
                                        .size(11.0)
                                        .color(p.text_secondary),
                                );
                                if ui
                                    .add_enabled_ui(self.selection.is_some(), |ui| {
                                        bevel_button(
                                            ui,
                                            t("Rauschen lernen", "Learn noise"),
                                            BevelStyle::Primary,
                                        )
                                    })
                                    .inner
                                    .clicked()
                                {
                                    self.learn_noise(&mut settings, &left, &right, rate, track);
                                }
                                egui::ComboBox::from_id_salt("song_noise_preset")
                                    .selected_text(t("Profil …", "Profile …"))
                                    .show_ui(ui, |ui| {
                                        for profile in presets {
                                            if ui.selectable_label(false, &profile.name).clicked() {
                                                settings.noise_profile = Some(profile.clone());
                                            }
                                        }
                                        if legacy.noise_profile.is_some()
                                            && ui
                                                .selectable_label(
                                                    false,
                                                    t(
                                                        "Aktuelles Aufnahmeprofil",
                                                        "Current recording profile",
                                                    ),
                                                )
                                                .clicked()
                                        {
                                            settings.noise_profile = legacy.noise_profile.clone();
                                        }
                                    });
                                if settings.noise_profile.is_some()
                                    && bevel_button(
                                        ui,
                                        t("Entfernen", "Remove"),
                                        BevelStyle::Neutral,
                                    )
                                    .clicked()
                                {
                                    settings.noise_profile = None;
                                }
                                if bevel_button(
                                    ui,
                                    t("Laden …", "Load …"),
                                    BevelStyle::Neutral,
                                )
                                .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Noise profile", &["json"])
                                        .pick_file()
                                    {
                                        match std::fs::read(&path)
                                            .map_err(|e| e.to_string())
                                            .and_then(|data| {
                                                serde_json::from_slice::<NoiseProfile>(&data)
                                                    .map_err(|e| e.to_string())
                                            }) {
                                            Ok(profile) => settings.noise_profile = Some(profile),
                                            Err(error) => self.error = Some(error),
                                        }
                                    }
                                }
                            });
                        }
                    });
        });
        let changed = settings != before || adopt;
        if changed {
            if self.edit_transaction.is_none() {
                self.edit_transaction = Some(splitter.snapshot());
            }
            splitter
                .song_settings
                .insert(track.segment_index, settings.clone());
        }
        if !ui.input(|input| input.pointer.primary_down()) {
            if let Some(snapshot) = self.edit_transaction.take() {
                splitter.commit_edit(snapshot);
            }
        }
        self.update_monitor(&settings);
        if self.player.is_playing() {
            ui.ctx().request_repaint();
        }
        export_song.then_some(SongEditorAction::Export(track.segment_index))
    }

    fn update_monitor(&self, settings: &SongSettings) {
        if self.original {
            self.player.set_song_settings(&SongSettings::default());
        } else {
            self.player.set_song_settings(settings);
        }
        self.player.set_song_monitor(self.solo, self.mono_audition);
    }
}

fn mono_waveform(left: &[f32], right: &[f32], settings: &SongSettings) -> Vec<StereoWaveformPoint> {
    let frames = left.len().min(right.len());
    let bins = frames.min(65_536);
    (0..bins)
        .map(|bin| {
            let mut point = StereoWaveformPoint::default();
            for (&l, &r) in left[bin * frames / bins..(bin + 1) * frames / bins]
                .iter()
                .zip(&right[bin * frames / bins..(bin + 1) * frames / bins])
            {
                let mono = match settings.routing {
                    Routing::LeftMono => l,
                    Routing::RightMono => r,
                    _ => l * settings.mix_left + r * settings.mix_right,
                };
                point.min_l = point.min_l.min(mono);
                point.max_l = point.max_l.max(mono);
            }
            point.min_r = point.min_l;
            point.max_r = point.max_l;
            point
        })
        .collect()
}

/// Link only the controls touched by this gesture. Previously independent
/// values on the other channel keep their settings.
fn link_changed_controls(
    before: &ChannelSettings,
    edited: &ChannelSettings,
    other: &mut ChannelSettings,
) {
    macro_rules! link {
        ($($($field:ident).+),+ $(,)?) => {
            $(if before.$($field).+ != edited.$($field).+ {
                other.$($field).+ = edited.$($field).+;
            })+
        };
    }
    link!(
        gain_db,
        muted,
        invert,
        eq.enabled,
        eq.low_db,
        eq.mid_db,
        eq.high_db,
        eq.low_hz,
        eq.mid_hz,
        eq.high_hz,
        eq.mid_q,
        denoiser.enabled,
        denoiser.reduction_db,
        denoiser.sensitivity,
        denoiser.smoothing,
        denoiser.dc_filter_enabled,
        denoiser.dc_cutoff_hz,
        compressor.enabled,
        compressor.threshold_db,
        compressor.ratio,
        compressor.attack_ms,
        compressor.release_ms,
        compressor.knee_db,
        compressor.makeup_db,
        expander.enabled,
        expander.threshold_db,
        expander.ratio,
        expander.attack_ms,
        expander.release_ms,
        expander.knee_db,
        expander.makeup_db,
        expander.upward,
        limiter.enabled,
        limiter.ceiling_db,
        limiter.drive_db,
        limiter.release_ms,
    );
}

fn channel_controls(
    ui: &mut Ui,
    channel: &mut ChannelSettings,
    module: Module,
    dynamics_unit: &mut DynamicsUnit,
    rack_h: f32,
    knob_size: f32,
    lang: Language,
) {
    let t = |de, en| if lang == Language::De { de } else { en };
    let p = palette();
    if module == Module::Channels {
        rack_columns(ui, 2, |cols| {
            module_card(
                &mut cols[0],
                "01",
                t("Eingangspegel", "Input gain"),
                t("Kanal-Trim", "Channel trim"),
                p.accent_top,
                |ui| {
                    rotary_colored_sized(
                        ui,
                        t("Kanalpegel", "Channel gain"),
                        &mut channel.gain_db,
                        -24.0..=12.0,
                        " dB",
                        p.accent_top,
                        knob_size,
                    );
                },
            );
            module_card(
                &mut cols[1],
                "02",
                t("Taktile Kanalschalter", "Channel switches"),
                t("Stumm & Polarität", "Mute & polarity"),
                p.teal,
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if bevel_button_sized(
                            ui,
                            if channel.muted {
                                t("● MUTE (Aktiv)", "● MUTE (Active)")
                            } else {
                                t("● MUTE (Stumm)", "● MUTE")
                            },
                            132.0,
                            if channel.muted {
                                BevelStyle::Record
                            } else {
                                BevelStyle::Neutral
                            },
                        )
                        .clicked()
                        {
                            channel.muted = !channel.muted;
                        }
                        if bevel_button_sized(
                            ui,
                            t("Ø Phase Invert", "Ø Phase invert"),
                            132.0,
                            if channel.invert {
                                BevelStyle::Engaged
                            } else {
                                BevelStyle::Neutral
                            },
                        )
                        .clicked()
                        {
                            channel.invert = !channel.invert;
                        }
                    });
                },
            );
        });
    }
    if module == Module::Restoration {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut channel.denoiser.enabled,
                t("Rauschfilter aktiv", "Noise reduction enabled"),
            );
            ui.checkbox(
                &mut channel.denoiser.dc_filter_enabled,
                t("DC / Subsonic aktiv", "DC / subsonic enabled"),
            );
        });
        ui.add_space(3.0);
        let rest_k = knob_size.min(36.0);
        rack_columns(ui, 4, |cols| {
            module_card(
                &mut cols[0],
                "01",
                t("Reduktion", "Reduction"),
                t("Hiss-Absenkung", "Hiss attenuation"),
                p.accent_top,
                |ui| {
                    ui.add_enabled_ui(channel.denoiser.enabled, |ui| {
                        rotary_colored_sized(
                            ui,
                            t("Reduktion", "Reduction"),
                            &mut channel.denoiser.reduction_db,
                            0.0..=40.0,
                            " dB",
                            p.accent_top,
                            rest_k,
                        );
                    });
                },
            );
            module_card(
                &mut cols[1],
                "02",
                t("Empfindlichkeit", "Sensitivity"),
                t("Noise-Floor-Erkennung", "Noise floor detect"),
                p.accent_top,
                |ui| {
                    ui.add_enabled_ui(channel.denoiser.enabled, |ui| {
                        rotary_colored_sized(
                            ui,
                            t("Schwelle", "Sensitivity"),
                            &mut channel.denoiser.sensitivity,
                            0.5..=4.0,
                            " ×",
                            p.accent_top,
                            rest_k,
                        );
                    });
                },
            );
            module_card(
                &mut cols[2],
                "03",
                t("Glättung", "Smoothing"),
                t("Transienten-Schutz", "Transient protect"),
                p.teal,
                |ui| {
                    ui.add_enabled_ui(channel.denoiser.enabled, |ui| {
                        rotary_colored_sized(
                            ui,
                            t("Glättung", "Smoothing"),
                            &mut channel.denoiser.smoothing,
                            0.0..=0.95,
                            "",
                            p.teal,
                            rest_k,
                        );
                    });
                },
            );
            module_card(
                &mut cols[3],
                "04",
                "Subsonic Hz",
                t("Rumpelfilter / DC Cut", "Rumble / DC cut"),
                p.amber,
                |ui| {
                    ui.add_enabled_ui(channel.denoiser.dc_filter_enabled, |ui| {
                        rotary_colored_sized(
                            ui,
                            t("Grenzfrequenz", "Cutoff"),
                            &mut channel.denoiser.dc_cutoff_hz,
                            5.0..=80.0,
                            " Hz",
                            p.amber,
                            rest_k,
                        );
                    });
                },
            );
        });
    }
    if module == Module::Equalizer {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut channel.eq.enabled, t("Equalizer aktiv", "Enable equalizer"));
            if bevel_button(ui, t("Flat", "Flat"), BevelStyle::Compact)
                .on_hover_text(t("Kurve zurücksetzen (Flat)", "Reset curve to flat"))
                .clicked()
            {
                channel.eq.low_db = 0.0;
                channel.eq.mid_db = 0.0;
                channel.eq.high_db = 0.0;
            }
            ui.label(
                egui::RichText::new(format!(
                    "1: {:+.1} dB @ {:.0} Hz  ·  2: {:+.1} dB @ {:.0} Hz (Q {:.1})  ·  3: {:+.1} dB @ {:.0} Hz",
                    channel.eq.low_db,
                    channel.eq.low_hz,
                    channel.eq.mid_db,
                    channel.eq.mid_hz,
                    channel.eq.mid_q,
                    channel.eq.high_db,
                    channel.eq.high_hz,
                ))
                .monospace()
                .size(10.5)
                .color(p.text_secondary),
            );
        });
        ui.add_space(4.0);
        ui.add_enabled_ui(channel.eq.enabled, |ui| {
            let eq_h = (rack_h - 210.0).clamp(48.0, 160.0);
            draw_eq_response_curve(ui, &mut channel.eq, eq_h);
            ui.add_space(4.0);
            let eq_k = knob_size.min(38.0);
            rack_columns(ui, 3, |cols| {
                module_card(
                    &mut cols[0],
                    "01",
                    t("Tiefen (Low Shelf)", "Low shelf"),
                    t("Analog Baxandall Typ", "Analog shelf"),
                    p.accent_top,
                    |ui| {
                        ui.columns(2, |pair| {
                            rotary_colored_sized(&mut pair[0], "Gain", &mut channel.eq.low_db, -18.0..=18.0, " dB", p.accent_top, eq_k);
                            rotary_colored_sized(&mut pair[1], t("Frequenz", "Freq"), &mut channel.eq.low_hz, 20.0..=500.0, " Hz", p.accent_top, eq_k);
                        });
                    },
                );
                module_card(
                    &mut cols[1],
                    "02",
                    t("Mitten (Parametric Peak)", "Mid band (Peak)"),
                    t("Symmetrischer Glockenfilter", "Symmetrical bell"),
                    p.teal,
                    |ui| {
                        ui.columns(3, |trio| {
                            rotary_colored_sized(&mut trio[0], "Gain", &mut channel.eq.mid_db, -18.0..=18.0, " dB", p.teal, eq_k);
                            rotary_colored_sized(&mut trio[1], "Freq", &mut channel.eq.mid_hz, 150.0..=8000.0, " Hz", p.teal, eq_k);
                            rotary_colored_sized(&mut trio[2], "Q", &mut channel.eq.mid_q, 0.2..=8.0, "", p.teal, eq_k);
                        });
                    },
                );
                module_card(
                    &mut cols[2],
                    "03",
                    t("Höhen (High Shelf)", "High shelf"),
                    t("Seidiger Band-Glanz", "Silky air shelf"),
                    p.amber,
                    |ui| {
                        ui.columns(2, |pair| {
                            rotary_colored_sized(&mut pair[0], "Gain", &mut channel.eq.high_db, -18.0..=18.0, " dB", p.amber, eq_k);
                            rotary_colored_sized(&mut pair[1], t("Frequenz", "Freq"), &mut channel.eq.high_hz, 1500.0..=20_000.0, " Hz", p.amber, eq_k);
                        });
                    },
                );
            });
        });
    }
    if module == Module::Dynamics {
        ui.horizontal_wrapped(|ui| {
            segmented_control(
                ui,
                "dynamics_unit_select",
                dynamics_unit,
                &[
                    (DynamicsUnit::Compressor, "Compressor", ""),
                    (DynamicsUnit::Expander, "Expander", ""),
                    (DynamicsUnit::Limiter, "Limiter", ""),
                ],
                HEIGHT_CONTROL,
            );
            match *dynamics_unit {
                DynamicsUnit::Compressor => {
                    ui.checkbox(&mut channel.compressor.enabled, t("Aktiv", "Enabled"));
                }
                DynamicsUnit::Expander => {
                    ui.checkbox(&mut channel.expander.enabled, t("Aktiv", "Enabled"));
                    segmented_control(
                        ui,
                        "expander_direction",
                        &mut channel.expander.upward,
                        &[
                            (false, t("Downward · Absenken", "Downward · lower"), ""),
                            (true, t("Upward · Anheben", "Upward · raise"), ""),
                        ],
                        HEIGHT_CONTROL,
                    );
                }
                DynamicsUnit::Limiter => {
                    ui.checkbox(&mut channel.limiter.enabled, t("Aktiv", "Enabled"));
                    ui.label(
                        egui::RichText::new(t(
                            "Brickwall · 0 ms Attack · True-Peak-Decke",
                            "Brickwall · 0 ms attack · peak ceiling",
                        ))
                        .size(10.5)
                        .color(p.text_muted),
                    );
                }
            }
        });
        ui.add_space(4.0);
        match *dynamics_unit {
            DynamicsUnit::Compressor => {
                dynamics_controls(ui, &mut channel.compressor, false, rack_h, lang);
            }
            DynamicsUnit::Expander => {
                dynamics_controls(ui, &mut channel.expander, true, rack_h, lang);
            }
            DynamicsUnit::Limiter => {
                limiter_controls(ui, &mut channel.limiter, rack_h, knob_size, lang);
            }
        }
    }
}

fn limiter_controls(
    ui: &mut Ui,
    params: &mut LimiterSettings,
    rack_h: f32,
    knob_size: f32,
    lang: Language,
) {
    let t = |de, en| if lang == Language::De { de } else { en };
    let p = palette();
    let lim_k = knob_size.clamp(40.0, 52.0);
    let curve_h = (rack_h - 120.0).clamp(92.0, 180.0);
    ui.add_enabled_ui(params.enabled, |ui| {
        let total_w = ui.available_width();
        let left_w = 164.0_f32;
        let right_w = (total_w - left_w - 22.0).max(240.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    module_card(
                        ui,
                        "01",
                        t("Kennlinie", "Curve"),
                        "Brickwall Limiter",
                        LED_RED,
                        |ui| {
                            draw_limiter_transfer_curve(ui, params, curve_h);
                        },
                    );
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(right_w, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    module_card(
                        ui,
                        "02",
                        t("Limiter-Parameter", "Limiter parameters"),
                        t("Spitzenpegel-Begrenzer", "Brickwall peak control"),
                        LED_RED,
                        |ui| {
                            rack_columns(ui, 3, |cols| {
                                rotary_colored_sized(
                                    &mut cols[0],
                                    "Ceiling",
                                    &mut params.ceiling_db,
                                    -24.0..=0.0,
                                    " dB",
                                    LED_RED,
                                    lim_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[1],
                                    "Drive",
                                    &mut params.drive_db,
                                    0.0..=24.0,
                                    " dB",
                                    p.accent_top,
                                    lim_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[2],
                                    "Release",
                                    &mut params.release_ms,
                                    5.0..=500.0,
                                    " ms",
                                    p.teal,
                                    lim_k,
                                );
                            });
                        },
                    );
                },
            );
        });
    });
}

fn draw_limiter_transfer_curve(ui: &mut Ui, params: &LimiterSettings, height: f32) {
    let p = palette();
    let size = egui::vec2(128.0, height.max(84.0));
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let pad = 10.0;
    let inner = rect.shrink(pad);
    let map = |in_db: f32, out_db: f32| {
        let nx = ((in_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let ny = ((out_db + 60.0) / 60.0).clamp(0.0, 1.0);
        egui::pos2(
            inner.min.x + nx * inner.width(),
            inner.max.y - ny * inner.height(),
        )
    };

    // 1:1 Referenzlinie
    painter.line_segment(
        [map(-60.0, -60.0), map(0.0, 0.0)],
        egui::Stroke::new(1.0, p.border_mid),
    );

    // Horizontale Ceiling-Markierung
    let ceil_y = map(0.0, params.ceiling_db).y;
    painter.line_segment(
        [egui::pos2(inner.min.x, ceil_y), egui::pos2(inner.max.x, ceil_y)],
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(LED_RED.r(), LED_RED.g(), LED_RED.b(), 110),
        ),
    );

    // Brickwall-Kennlinie: out = min(in + drive, ceiling)
    let mut pts = Vec::with_capacity(61);
    for i in 0..=60 {
        let in_db = -60.0 + i as f32;
        let out_db = (in_db + params.drive_db).min(params.ceiling_db);
        pts.push(map(in_db, out_db));
    }
    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.2, LED_RED)));

    // Kniepunkt (Eingangspegel, ab dem begrenzt wird)
    let knee_in = (params.ceiling_db - params.drive_db).clamp(-60.0, 0.0);
    let knee_pt = map(knee_in, params.ceiling_db);
    painter.circle_filled(knee_pt, 3.5, LED_RED);
    painter.text(
        egui::pos2(inner.min.x + 2.0, inner.min.y + 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.1} dB", params.ceiling_db),
        egui::FontId::monospace(8.5),
        LED_RED,
    );
}

fn dynamics_controls(
    ui: &mut Ui,
    params: &mut DynamicsSettings,
    expander: bool,
    rack_h: f32,
    lang: Language,
) {
    let t = |de, en| if lang == Language::De { de } else { en };
    let p = palette();
    let dyn_k = if rack_h < 285.0 {
        34.0
    } else if rack_h < 340.0 {
        40.0
    } else {
        46.0
    };
    let curve_h = (rack_h - 130.0).clamp(88.0, 180.0);
    ui.add_enabled_ui(params.enabled, |ui| {
        let total_w = ui.available_width();
        let left_w = 164.0_f32;
        let right_w = (total_w - left_w - 22.0).max(240.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    module_card(
                        ui,
                        "01",
                        t("Kennlinie", "Curve"),
                        if expander { "Expander" } else { "Compressor" },
                        p.accent_top,
                        |ui| {
                            draw_dynamics_transfer_curve(ui, params, expander, curve_h);
                        },
                    );
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(right_w, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    module_card(
                        ui,
                        "02",
                        t("Parameter-Matrix", "Parameter matrix"),
                        "High-Res Rotary Attenuators",
                        p.teal,
                        |ui| {
                            rack_columns(ui, 3, |cols| {
                                rotary_colored_sized(
                                    &mut cols[0],
                                    "Threshold",
                                    &mut params.threshold_db,
                                    -80.0..=0.0,
                                    " dB",
                                    p.accent_top,
                                    dyn_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[1],
                                    "Ratio",
                                    &mut params.ratio,
                                    1.0..=20.0,
                                    ":1",
                                    p.accent_top,
                                    dyn_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[2],
                                    "Knee",
                                    &mut params.knee_db,
                                    0.0..=24.0,
                                    " dB",
                                    p.amber,
                                    dyn_k,
                                );
                            });
                            ui.add_space(4.0);
                            rack_columns(ui, 3, |cols| {
                                rotary_colored_sized(
                                    &mut cols[0],
                                    "Attack",
                                    &mut params.attack_ms,
                                    0.1..=200.0,
                                    " ms",
                                    p.teal,
                                    dyn_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[1],
                                    "Release",
                                    &mut params.release_ms,
                                    1.0..=2000.0,
                                    " ms",
                                    p.teal,
                                    dyn_k,
                                );
                                rotary_colored_sized(
                                    &mut cols[2],
                                    t("Ausgang", "Output"),
                                    &mut params.makeup_db,
                                    -24.0..=24.0,
                                    " dB",
                                    p.amber,
                                    dyn_k,
                                );
                            });
                        },
                    );
                },
            );
        });
    });
}

/// Dunkles OLED-Spektrum-Display wie im Stitch Restaurations-Panel (Amber-Rauschkurve + Teal-Zielkurve + Subsonic-Cut).
fn draw_noise_spectrum_curve(
    ui: &mut Ui,
    profile: Option<&NoiseProfile>,
    reduction_db: f32,
    subsonic_hz: Option<f32>,
    height: f32,
    lang: Language,
) {
    let desired = egui::vec2(ui.available_width().max(260.0), height.max(40.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();
    let oled_bg = egui::Color32::from_rgb(13, 17, 23);
    painter.rect_filled(rect, RADIUS_WELL, oled_bg);
    painter.rect_stroke(
        rect.shrink(0.5),
        RADIUS_WELL,
        egui::Stroke::new(0.75, egui::Color32::from_rgb(44, 50, 62)),
    );

    // Subtiles OLED-Messgitter
    for frac in [0.25_f32, 0.5, 0.75] {
        let y = rect.min.y + rect.height() * frac;
        painter.line_segment(
            [egui::pos2(rect.min.x + 8.0, y), egui::pos2(rect.max.x - 8.0, y)],
            egui::Stroke::new(0.75, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14)),
        );
    }

    let Some(profile) = profile.filter(|prof| !prof.noise_power_l.is_empty()) else {
        let msg = if lang == Language::De {
            "Kein Rauschprofil geladen — Abschnitt in der Wellenform wählen und „Rauschen aus Auswahl lernen“ klicken"
        } else {
            "No noise profile loaded — select a noise-only range in the waveform and click Learn noise"
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(11.2),
            egui::Color32::from_rgb(148, 163, 184),
        );
        return;
    };

    let bins = profile.noise_power_l.len();
    let steps = 72usize;
    let pad_x = 12.0;
    let pad_y = 12.0;
    let inner_w = (rect.width() - pad_x * 2.0).max(10.0);
    let inner_h = (rect.height() - pad_y * 2.0).max(10.0);
    let bottom_y = rect.max.y - pad_y;

    let mut raw_pts = Vec::with_capacity(steps);
    let mut reduced_pts = Vec::with_capacity(steps);
    for i in 0..steps {
        let frac = i as f32 / (steps - 1) as f32;
        let bin_f = ((bins as f32).powf(frac) - 1.0).clamp(0.0, (bins - 1) as f32);
        let idx = bin_f as usize;
        let power = profile.noise_power_l[idx].max(1e-12);
        let db = (10.0 * power.log10()).clamp(-100.0, 0.0);
        let norm_raw = ((db + 100.0) / 100.0).clamp(0.0, 1.0);
        let norm_red = (((db - reduction_db) + 100.0) / 100.0).clamp(0.0, 1.0);
        let x = rect.min.x + pad_x + frac * inner_w;
        raw_pts.push(egui::pos2(x, bottom_y - norm_raw * inner_h));
        reduced_pts.push(egui::pos2(x, bottom_y - norm_red * inner_h));
    }

    // Transluzente Kurven-Flächen
    let mut raw_poly = raw_pts.clone();
    raw_poly.push(egui::pos2(rect.max.x - pad_x, bottom_y));
    raw_poly.push(egui::pos2(rect.min.x + pad_x, bottom_y));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(rect.min.x + pad_x, bottom_y),
            raw_pts[0],
            *raw_pts.last().unwrap(),
            egui::pos2(rect.max.x - pad_x, bottom_y),
        ],
        egui::Color32::from_rgba_unmultiplied(217, 119, 6, 26),
        egui::Stroke::NONE,
    ));

    // Vertikale Subsonic-Cut-Linie (wie im Stitch-Panel)
    if let Some(hz) = subsonic_hz {
        let log_min = 20.0_f32.ln();
        let log_max = 20_000.0_f32.ln();
        let f = ((hz.clamp(20.0, 20_000.0).ln() - log_min) / (log_max - log_min)).clamp(0.0, 1.0);
        let sx = rect.min.x + pad_x + f * inner_w;
        painter.line_segment(
            [egui::pos2(sx, rect.min.y + 8.0), egui::pos2(sx, bottom_y)],
            egui::Stroke::new(1.2, egui::Color32::from_rgb(245, 158, 11)),
        );
        painter.text(
            egui::pos2(sx + 5.0, bottom_y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{hz:.0} Hz"),
            egui::FontId::monospace(9.0),
            egui::Color32::from_rgb(245, 158, 11),
        );
    }

    painter.add(egui::Shape::line(
        raw_pts,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(234, 128, 36)),
    ));
    painter.add(egui::Shape::line(
        reduced_pts,
        egui::Stroke::new(1.8, egui::Color32::from_rgb(20, 184, 166)),
    ));

    // Teal-Badge "-X.X dB Hiss"
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - 102.0, rect.min.y + 8.0),
        egui::vec2(92.0, 18.0),
    );
    painter.rect_filled(badge_rect, 4.0, egui::Color32::from_rgb(13, 148, 136));
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("-{reduction_db:.1} dB Hiss"),
        egui::FontId::monospace(9.5),
        egui::Color32::WHITE,
    );
    painter.text(
        egui::pos2(rect.min.x + 12.0, rect.min.y + 8.0),
        egui::Align2::LEFT_TOP,
        format!("● Live FFT · {}", profile.name),
        egui::FontId::monospace(9.5),
        egui::Color32::from_rgb(148, 163, 184),
    );
}

fn draw_dynamics_transfer_curve(
    ui: &mut Ui,
    params: &DynamicsSettings,
    expander: bool,
    height: f32,
) {
    let p = palette();
    let size = egui::vec2(128.0, height.max(84.0));
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let pad = 10.0;
    let inner = rect.shrink(pad);
    let map = |in_db: f32, out_db: f32| {
        let nx = ((in_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let ny = ((out_db + 60.0) / 60.0).clamp(0.0, 1.0);
        egui::pos2(
            inner.min.x + nx * inner.width(),
            inner.max.y - ny * inner.height(),
        )
    };

    // 1:1 Referenzdiagonale
    painter.line_segment(
        [map(-60.0, -60.0), map(0.0, 0.0)],
        egui::Stroke::new(1.0, p.border_mid),
    );

    // Kennlinie berechnen
    let th = params.threshold_db.clamp(-60.0, 0.0);
    let ratio = params.ratio.max(1.0);
    let mut pts = Vec::with_capacity(48);
    for i in 0..48 {
        let in_db = -60.0 + (i as f32 / 47.0) * 60.0;
        let out_db = if !expander {
            if in_db > th {
                th + (in_db - th) / ratio
            } else {
                in_db
            }
        } else if params.upward {
            if in_db > th {
                th + (in_db - th) * ratio.min(4.0)
            } else {
                in_db
            }
        } else if in_db < th {
            th - (th - in_db) * ratio.min(4.0)
        } else {
            in_db
        };
        pts.push(map(in_db, out_db.clamp(-60.0, 0.0)));
    }
    // Schattierte Kompressions-/Expansions-Fläche zwischen 1:1-Linie und Kennlinie
    let shade = egui::Color32::from_rgba_unmultiplied(
        p.accent_top.r(),
        p.accent_top.g(),
        p.accent_top.b(),
        if p.is_dark { 46 } else { 36 },
    );
    for i in 0..(pts.len() - 1) {
        let in0 = -60.0 + (i as f32 / 47.0) * 60.0;
        let in1 = -60.0 + ((i + 1) as f32 / 47.0) * 60.0;
        let d0 = map(in0, in0);
        let d1 = map(in1, in1);
        if (pts[i].y - d0.y).abs() > 0.5 || (pts[i + 1].y - d1.y).abs() > 0.5 {
            painter.add(egui::Shape::convex_polygon(
                vec![d0, d1, pts[i + 1], pts[i]],
                shade,
                egui::Stroke::NONE,
            ));
        }
    }
    painter.add(egui::Shape::line(
        pts,
        egui::Stroke::new(2.2, p.accent_top),
    ));

    // Threshold-Punkt markieren
    let th_pt = map(th, th);
    painter.circle_filled(th_pt, 3.5, p.teal);
    painter.text(
        egui::pos2(rect.min.x + 7.0, rect.min.y + 5.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0} dB · {:.1}:1", params.threshold_db, params.ratio),
        egui::FontId::monospace(9.0),
        p.text_muted,
    );
}

/// Interaktive 3-Band-EQ-Frequenzgangkurve (20 Hz – 20 kHz, -18..+18 dB) mit 3 direkt ziehbaren Knoten.
fn draw_eq_response_curve(
    ui: &mut Ui,
    eq: &mut crate::audio::processing::EqSettings,
    height: f32,
) {
    let p = palette();
    let desired = egui::vec2(ui.available_width().max(280.0), height.max(48.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();
    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let pad_l = 30.0;
    let pad_r = 12.0;
    let pad_t = 12.0;
    let pad_b = 18.0;
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + pad_l, rect.min.y + pad_t),
        egui::pos2(rect.max.x - pad_r, rect.max.y - pad_b),
    );

    let log_min = 20.0_f32.ln();
    let log_max = 20_000.0_f32.ln();
    let freq_to_x = |hz: f32| -> f32 {
        let f = ((hz.clamp(20.0, 20_000.0).ln() - log_min) / (log_max - log_min)).clamp(0.0, 1.0);
        inner.min.x + f * inner.width()
    };
    let x_to_freq = |x: f32| -> f32 {
        let f = ((x - inner.min.x) / inner.width().max(1.0)).clamp(0.0, 1.0);
        (log_min + f * (log_max - log_min)).exp()
    };
    let db_to_y = |db: f32| -> f32 {
        let f = ((db.clamp(-18.0, 18.0) + 18.0) / 36.0).clamp(0.0, 1.0);
        inner.max.y - f * inner.height()
    };
    let y_to_db = |y: f32| -> f32 {
        let f = ((inner.max.y - y) / inner.height().max(1.0)).clamp(0.0, 1.0);
        -18.0 + f * 36.0
    };

    // Frequenz-Gitter
    for &(hz, lbl) in &[
        (50.0, "50"),
        (100.0, "100"),
        (250.0, "250"),
        (1000.0, "1k"),
        (4000.0, "4k"),
        (10000.0, "10k"),
    ] {
        let x = freq_to_x(hz);
        painter.line_segment(
            [egui::pos2(x, inner.min.y), egui::pos2(x, inner.max.y)],
            egui::Stroke::new(0.75, p.border_subtle),
        );
        painter.text(
            egui::pos2(x, inner.max.y + 3.0),
            egui::Align2::CENTER_TOP,
            lbl,
            egui::FontId::monospace(8.5),
            p.text_muted,
        );
    }

    // dB-Gitter (+12, 0, -12)
    for &(db, lbl) in &[(12.0, "+12"), (0.0, "0"), (-12.0, "-12")] {
        let y = db_to_y(db);
        painter.line_segment(
            [egui::pos2(inner.min.x, y), egui::pos2(inner.max.x, y)],
            egui::Stroke::new(if db == 0.0 { 1.1 } else { 0.75 }, if db == 0.0 { p.border_mid } else { p.border_subtle }),
        );
        painter.text(
            egui::pos2(inner.min.x - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            lbl,
            egui::FontId::monospace(8.5),
            p.text_muted,
        );
    }

    // Summen-Frequenzgang (Low-Shelf + Mid-Peak + High-Shelf)
    let eval_db = |hz: f32| -> f32 {
        let low_ratio = (hz / eq.low_hz.max(20.0)).ln();
        let low_weight = 1.0 / (1.0 + (low_ratio * 2.2).exp());
        let high_ratio = (eq.high_hz.max(200.0) / hz).ln();
        let high_weight = 1.0 / (1.0 + (high_ratio * 2.2).exp());
        let mid_oct = (hz / eq.mid_hz.max(40.0)).log2();
        let bw = (1.15 / eq.mid_q.clamp(0.2, 8.0)).max(0.15);
        let mid_weight = (-0.5 * (mid_oct / bw).powi(2)).exp();
        (eq.low_db * low_weight + eq.mid_db * mid_weight + eq.high_db * high_weight).clamp(-18.0, 18.0)
    };

    let steps = 96usize;
    let mut pts = Vec::with_capacity(steps);
    for i in 0..steps {
        let f = i as f32 / (steps - 1) as f32;
        let hz = (log_min + f * (log_max - log_min)).exp();
        pts.push(egui::pos2(freq_to_x(hz), db_to_y(eval_db(hz))));
    }
    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0, p.accent_top)));

    // 3 direkt ziehbare Band-Knoten
    let enabled = ui.is_enabled();
    let bands = [
        ("1", &mut eq.low_hz, &mut eq.low_db, 20.0_f32..=500.0, p.accent_top),
        ("2", &mut eq.mid_hz, &mut eq.mid_db, 150.0_f32..=8000.0, p.teal),
        ("3", &mut eq.high_hz, &mut eq.high_db, 1500.0_f32..=20_000.0, p.amber),
    ];
    for (idx, (tag, hz, db, hz_range, col)) in bands.into_iter().enumerate() {
        let pos = egui::pos2(freq_to_x(*hz), db_to_y(*db));
        let hit = egui::Rect::from_center_size(pos, egui::vec2(20.0, 20.0));
        let resp = ui.interact(hit, ui.id().with(("eq_node", idx)), egui::Sense::click_and_drag());
        if enabled && resp.dragged() {
            if let Some(pointer) = resp.interact_pointer_pos() {
                *hz = x_to_freq(pointer.x).clamp(*hz_range.start(), *hz_range.end());
                *db = y_to_db(pointer.y).clamp(-18.0, 18.0);
            }
        }
        let r = if resp.hovered() || resp.dragged() { 8.5 } else { 7.5 };
        painter.circle_filled(pos, r, col);
        painter.circle_stroke(pos, r, egui::Stroke::new(1.2, p.bg_panel));
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            tag,
            egui::FontId::monospace(9.0),
            egui::Color32::WHITE,
        );
        resp.on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text(format!("{tag}: {:.0} Hz · {:+.1} dB", *hz, *db));
    }
}

fn routing_label(routing: Routing, lang: Language) -> &'static str {
    match (routing, lang) {
        (Routing::Stereo, _) => "Stereo",
        (Routing::LeftMono, Language::De) => "Nur Links zu Mono",
        (Routing::RightMono, Language::De) => "Nur Rechts zu Mono",
        (Routing::MonoMix, Language::De) => "Links + Rechts zu Monomix",
        (Routing::LeftMono, _) => "Left only to mono",
        (Routing::RightMono, _) => "Right only to mono",
        (Routing::MonoMix, _) => "Left + Right to mono mix",
    }
}

fn export_label(format: SongExportMode, lang: Language) -> &'static str {
    match (format, lang) {
        (SongExportMode::AsEdited, Language::De) => "Export: Songformat (Mono / Stereo)",
        (SongExportMode::Stereo, Language::De) => "Export: Stereo-WAV (2 Kanäle)",
        (SongExportMode::SeparateMono, Language::De) => "Export: 2 Monospuren (L / R)",
        (SongExportMode::AsEdited, _) => "Export: Song format (mono / stereo)",
        (SongExportMode::Stereo, _) => "Export: Stereo WAV (2 channels)",
        (SongExportMode::SeparateMono, _) => "Export: 2 mono tracks (L / R)",
    }
}

fn timecode(seconds: f64) -> String {
    format!(
        "{:02}:{:06.3}",
        (seconds.max(0.0) / 60.0) as u64,
        seconds.max(0.0) % 60.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_gain_does_not_replace_the_other_channels_effect_settings() {
        let before = ChannelSettings::default();
        let mut left = before.clone();
        left.gain_db = 4.0;
        let mut right = before.clone();
        right.expander.enabled = true;
        right.eq.mid_db = -5.0;
        link_changed_controls(&before, &left, &mut right);
        assert_eq!(right.gain_db, 4.0);
        assert!(right.expander.enabled);
        assert_eq!(right.eq.mid_db, -5.0);
    }

    #[test]
    fn mono_waveform_mixes_samples_before_envelope_binning() {
        let settings = SongSettings {
            routing: Routing::MonoMix,
            ..Default::default()
        };
        let waveform = mono_waveform(&[0.5, -0.5], &[-0.5, 0.5], &settings);
        assert!(waveform
            .iter()
            .all(|point| point.min_l == 0.0 && point.max_l == 0.0));
    }

    #[test]
    fn selecting_another_song_resets_local_transport_and_selection() {
        let mut editor = SongEditor::default();
        let left = Arc::new(vec![0.1; 1000]);
        let right = left.clone();
        let mut splitter = TrackSplitter::new();
        splitter.markers = vec![5.0];
        let tracks = splitter.get_tracks(10.0);
        editor.prepare(left.clone(), right.clone(), 100, &tracks[0]);
        editor.player.seek_seconds(3.0);
        editor.selection = Some((1.0, 2.0));
        editor.view.zoom = 5.0;
        editor.prepare(left, right, 100, &tracks[1]);
        assert_eq!(editor.player.current_seconds(), 0.0);
        assert_eq!(editor.player.total_seconds(), 5.0);
        assert_eq!(editor.selection, None);
        assert_eq!(editor.view.zoom, 1.0);
        assert_eq!(editor.view.selected_segment, Some(1));
    }
}

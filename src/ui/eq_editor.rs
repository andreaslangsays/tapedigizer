use std::ops::RangeInclusive;

use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::audio::processing::EqSettings;
use crate::audio::spectrum::OutputSpectrum;
use crate::i18n::Language;

use super::theme::{palette, Palette, HEIGHT_CONTROL, RADIUS_WELL};
use super::widgets::button::{bevel_button, segmented_control, BevelStyle};
use super::widgets::paint::draw_recessed_well;
use super::widgets::panel::rack_columns;
use super::widgets::rotary::{rotary, RotaryAccent, RotarySize, RotarySpec};

const HZ: RangeInclusive<f32> = 20.0..=20_000.0;
const GAIN: RangeInclusive<f32> = -18.0..=18.0;
const Q: RangeInclusive<f32> = 0.2..=8.0;
const SPECTRUM_FLOOR: f32 = -90.0;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Band {
    LowShelf,
    Peak1,
    #[default]
    Mid,
    Peak2,
    HighShelf,
}

impl Band {
    const ALL: [Self; 5] = [
        Self::LowShelf,
        Self::Peak1,
        Self::Mid,
        Self::Peak2,
        Self::HighShelf,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::LowShelf => "1 · Low Shelf",
            Self::Peak1 => "2 · Peak",
            Self::Mid => "3 · Mid",
            Self::Peak2 => "4 · Peak",
            Self::HighShelf => "5 · High Shelf",
        }
    }

    fn number(self) -> &'static str {
        ["1", "2", "3", "4", "5"][self as usize]
    }

    fn range(self) -> RangeInclusive<f32> {
        match self {
            Self::LowShelf => 20.0..=500.0,
            Self::Mid => 150.0..=8_000.0,
            Self::HighShelf => 1500.0..=20_000.0,
            Self::Peak1 | Self::Peak2 => HZ,
        }
    }

    fn accent(self) -> RotaryAccent {
        match self {
            Self::LowShelf => RotaryAccent::Primary,
            Self::HighShelf => RotaryAccent::Amber,
            _ => RotaryAccent::Signal,
        }
    }

    fn color(self, p: Palette) -> Color32 {
        match self {
            Self::LowShelf => p.accent_top,
            Self::HighShelf => p.amber,
            _ => p.teal,
        }
    }

    fn values(self, eq: &EqSettings) -> (f32, f32) {
        match self {
            Self::LowShelf => (eq.low_hz, eq.low_db),
            Self::Peak1 => (eq.peaks[0].hz, eq.peaks[0].gain_db),
            Self::Mid => (eq.mid_hz, eq.mid_db),
            Self::Peak2 => (eq.peaks[1].hz, eq.peaks[1].gain_db),
            Self::HighShelf => (eq.high_hz, eq.high_db),
        }
    }

    fn controls(self, eq: &mut EqSettings) -> (&mut f32, &mut f32, Option<&mut f32>) {
        match self {
            Self::LowShelf => (&mut eq.low_hz, &mut eq.low_db, None),
            Self::Mid => (&mut eq.mid_hz, &mut eq.mid_db, Some(&mut eq.mid_q)),
            Self::HighShelf => (&mut eq.high_hz, &mut eq.high_db, None),
            Self::Peak1 | Self::Peak2 => {
                let peak = &mut eq.peaks[usize::from(self == Self::Peak2)];
                (&mut peak.hz, &mut peak.gain_db, Some(&mut peak.q))
            }
        }
    }
}

#[derive(Clone, Copy)]
struct NodeDrag {
    band: Band,
    offset: Vec2,
}

#[derive(Default)]
pub struct EqEditor {
    band: Band,
}

impl EqEditor {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        eq: &mut EqSettings,
        rate: u32,
        spectrum: Option<&OutputSpectrum>,
        height: f32,
        lang: Language,
    ) {
        let t = |de, en| if lang == Language::De { de } else { en };
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut eq.enabled, t("Equalizer aktiv", "Enable equalizer"));
            if bevel_button(ui, "Flat", BevelStyle::Compact)
                .on_hover_text(t("Alle Band-Pegel auf 0 dB", "Set all band gains to 0 dB"))
                .clicked()
            {
                for band in Band::ALL {
                    *band.controls(eq).1 = 0.0;
                }
            }
            ui.colored_label(
                palette(ui.ctx()).teal,
                t("Live · Ausgang (dBFS)", "Live · Output (dBFS)"),
            );
        });
        self.graph(ui, eq, rate, spectrum, height);
        ui.add_space(4.0);
        ui.add_enabled_ui(eq.enabled, |ui| {
            let options = Band::ALL.map(|band| (band, band.label(), ""));
            segmented_control(ui, "eq_band", &mut self.band, &options, HEIGHT_CONTROL);
            ui.add_space(4.0);

            let accent = self.band.accent();
            let hz_range = self.band.range();
            let (hz, gain, q) = self.band.controls(eq);
            rack_columns(ui, if q.is_some() { 3 } else { 2 }, |cols| {
                let size = RotarySize::Eq;
                rotary(
                    &mut cols[0],
                    hz,
                    RotarySpec::new(t("Frequenz", "Frequency"), hz_range, " Hz", accent, size),
                );
                rotary(
                    &mut cols[1],
                    gain,
                    RotarySpec::new("Gain", GAIN, " dB", accent, size),
                );
                if let Some(q) = q {
                    rotary(&mut cols[2], q, RotarySpec::new("Q", Q, "", accent, size));
                }
            });
        });
    }

    fn graph(
        &mut self,
        ui: &mut Ui,
        eq: &mut EqSettings,
        rate: u32,
        spectrum: Option<&OutputSpectrum>,
        height: f32,
    ) {
        let size = Vec2::new(ui.available_width(), height.max(82.0));
        let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
        let plot = Rect::from_min_max(
            rect.min + Vec2::new(31.0, 12.0),
            rect.max - Vec2::new(29.0, 18.0),
        );
        let p = palette(ui.ctx());
        draw_recessed_well(ui.painter(), rect, RADIUS_WELL, p.bg_inset);
        let hz_at = |fraction: f32| 20.0 * 1000.0_f32.powf(fraction);
        let x_at =
            |hz: f32| plot.left() + (hz.clamp(20.0, 20_000.0) / 20.0).log(1000.0) * plot.width();
        let y_at = |db: f32| plot.bottom() - (db.clamp(-18.0, 18.0) + 18.0) / 36.0 * plot.height();
        let point = |(hz, db)| Pos2::new(x_at(hz), y_at(db));
        let nearest = |eq: &EqSettings, pos: Pos2, selected: Band| {
            Band::ALL
                .into_iter()
                .map(|band| (band, point(band.values(eq)).distance_sq(pos)))
                .filter(|(_, distance)| *distance <= 15.0 * 15.0)
                .min_by(|(a, da), (b, db)| {
                    da.total_cmp(db)
                        .then_with(|| (*b == selected).cmp(&(*a == selected)))
                })
                .map(|(band, _)| band)
        };

        // The plot owns the gesture for its whole duration. Nodes may cross,
        // overlap, or move outside the hit area without losing pointer capture.
        let drag_id = response.id.with("node_drag");
        let dragging = eq.enabled && response.dragged_by(PointerButton::Primary);
        let finishing = eq.enabled && response.drag_stopped_by(PointerButton::Primary);
        let mut drag = ui.data(|data| data.get_temp::<NodeDrag>(drag_id));
        if dragging && response.drag_started_by(PointerButton::Primary) {
            drag = ui
                .input(|input| input.pointer.press_origin())
                .and_then(|start| {
                    let band = nearest(eq, start, self.band)?;
                    Some(NodeDrag {
                        band,
                        offset: point(band.values(eq)) - start,
                    })
                });
        }
        if let Some(gesture) = drag.filter(|_| dragging || finishing) {
            self.band = gesture.band;
            if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                let pos = pointer + gesture.offset;
                let (hz, db, _) = gesture.band.controls(eq);
                let dx = pos.x - x_at(*hz);
                let dy = pos.y - y_at(*db);
                if dx.abs() > 0.001 {
                    let range = gesture.band.range();
                    *hz = hz_at(((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0))
                        .clamp(*range.start(), *range.end());
                }
                if dy.abs() > 0.001 {
                    *db = (18.0 - (pos.y - plot.top()) / plot.height() * 36.0).clamp(-18.0, 18.0);
                }
            }
        }
        ui.data_mut(|data| match drag.filter(|_| dragging) {
            Some(drag) => data.insert_temp(drag_id, drag),
            None => data.remove::<NodeDrag>(drag_id),
        });

        let painter = ui.painter();
        if let Some(spectrum) = spectrum {
            draw_spectrum(ui, plot, spectrum, &hz_at, p);
        }
        for (hz, label) in [
            (50.0, "50"),
            (100.0, "100"),
            (250.0, "250"),
            (1000.0, "1k"),
            (4000.0, "4k"),
            (10_000.0, "10k"),
        ] {
            let x = x_at(hz);
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(0.75, p.border_subtle),
            );
            painter.text(
                Pos2::new(x, plot.bottom() + 3.0),
                Align2::CENTER_TOP,
                label,
                FontId::monospace(8.5),
                p.text_muted,
            );
        }
        for (db, label) in [(12.0, "+12"), (0.0, "0"), (-12.0, "-12")] {
            let y = y_at(db);
            painter.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(0.8, p.border_mid),
            );
            painter.text(
                Pos2::new(plot.left() - 4.0, y),
                Align2::RIGHT_CENTER,
                label,
                FontId::monospace(8.5),
                p.text_muted,
            );
        }

        let settings = eq.response(rate);
        let points = (0..160)
            .map(|step| {
                let hz = hz_at(step as f32 / 159.0);
                let gain = if eq.enabled {
                    settings.gain_db(hz)
                } else {
                    0.0
                };
                point((hz, gain))
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(2.0, p.accent_top)));

        let hovered = eq
            .enabled
            .then(|| {
                response
                    .hover_pos()
                    .and_then(|pos| nearest(eq, pos, self.band))
            })
            .flatten();
        let current_drag = drag.filter(|_| dragging);
        if current_drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        for band in Band::ALL
            .into_iter()
            .filter(|band| *band != self.band)
            .chain([self.band])
        {
            let values = band.values(eq);
            let pos = point(values);
            let selected = band == self.band;
            let radius = if selected || hovered == Some(band) {
                9.0
            } else {
                7.5
            };
            painter.circle(
                pos,
                radius,
                band.color(p),
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected { p.text_primary } else { p.bg_panel },
                ),
            );
            painter.text(
                pos,
                Align2::CENTER_CENTER,
                band.number(),
                FontId::monospace(9.0),
                p.text_on_bright_accent,
            );
        }
        if let Some(band) = current_drag.map(|gesture| gesture.band).or(hovered) {
            let (hz, db) = band.values(eq);
            let label = format!("{hz:.0} Hz · {db:+.1} dB");
            painter.text(
                plot.right_top() + Vec2::new(-4.0, 1.0),
                Align2::RIGHT_TOP,
                label,
                FontId::monospace(10.0),
                p.text_primary,
            );
        }
    }
}

fn draw_spectrum(
    ui: &Ui,
    plot: Rect,
    spectrum: &OutputSpectrum,
    hz_at: &impl Fn(f32) -> f32,
    p: Palette,
) {
    let painter = ui.painter();
    let bin_width = spectrum.sample_rate as f32 / (2 * (spectrum.db.len() - 1)) as f32;
    let columns = 128;
    for column in 0..columns {
        let lo = column as f32 / columns as f32;
        let hi = (column + 1) as f32 / columns as f32;
        let first = (hz_at(lo) / bin_width).round().max(1.0) as usize;
        if first >= spectrum.db.len() {
            break;
        }
        let last = ((hz_at(hi) / bin_width).round() as usize).clamp(first, spectrum.db.len() - 1);
        let db = spectrum.db[first..=last]
            .iter()
            .copied()
            .fold(SPECTRUM_FLOOR, f32::max);
        let level = (db.clamp(SPECTRUM_FLOOR, 0.0) - SPECTRUM_FLOOR) / -SPECTRUM_FLOOR;
        let top = plot.bottom() - level * plot.height();
        let bar = Rect::from_min_max(
            Pos2::new(plot.left() + lo * plot.width(), top),
            Pos2::new(plot.left() + hi * plot.width(), plot.bottom()),
        );
        painter.rect_filled(bar, 0.0, p.teal.gamma_multiply(0.35));
    }
    for (fraction, label) in [(0.0, "0"), (0.5, "−45"), (1.0, "−90")] {
        painter.text(
            Pos2::new(plot.right() + 4.0, plot.top() + fraction * plot.height()),
            Align2::LEFT_CENTER,
            label,
            FontId::monospace(8.5),
            p.teal,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, RawInput};

    #[test]
    fn band_drag_keeps_grab_offset_and_identity_through_another_node() {
        let ctx = egui::Context::default();
        let mut eq = EqSettings {
            enabled: true,
            ..Default::default()
        };
        let mut editor = EqEditor::default();
        let plot_rect = std::cell::Cell::new(Rect::NOTHING);
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 320.0));
        let frame = |editor: &mut EqEditor, eq: &mut EqSettings, events| {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let start = ui.next_widget_position();
                        plot_rect.set(Rect::from_min_max(
                            start + Vec2::new(31.0, 12.0),
                            start + Vec2::new(ui.available_width() - 29.0, 122.0),
                        ));
                        editor.graph(ui, eq, 48_000, None, 140.0);
                    });
                },
            );
        };
        frame(&mut editor, &mut eq, vec![]);
        let plot = plot_rect.get();
        let x = |hz: f32| plot.left() + (hz / 20.0).log(1000.0) * plot.width();
        let offset = Vec2::new(5.0, 3.0);
        let press = Pos2::new(x(350.0), plot.center().y) + offset;
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&mut editor, &mut eq, vec![Event::PointerMoved(press)]);
        frame(&mut editor, &mut eq, vec![button(press, true)]);
        assert!(editor.band == Band::Peak1);
        assert_eq!(eq.peaks[0].hz, 350.0);
        assert_eq!(eq.peaks[0].gain_db, 0.0);

        frame(
            &mut editor,
            &mut eq,
            vec![Event::PointerMoved(press - Vec2::new(0.0, 12.0))],
        );
        assert_eq!(eq.peaks[0].hz, 350.0);
        assert!(eq.peaks[0].gain_db > 3.0);

        let overlap = Pos2::new(x(3500.0), plot.center().y) + offset;
        frame(&mut editor, &mut eq, vec![Event::PointerMoved(overlap)]);
        assert!((eq.peaks[0].hz - 3500.0).abs() < 0.1);
        let end = Pos2::new(x(12_000.0), plot.center().y) + offset;
        frame(
            &mut editor,
            &mut eq,
            vec![Event::PointerMoved(end), button(end, false)],
        );
        assert!((eq.peaks[0].hz - 12_000.0).abs() < 0.2);
        assert_eq!(eq.peaks[1].hz, 3500.0);
        assert!(editor.band == Band::Peak1);
    }
}

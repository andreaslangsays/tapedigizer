use egui::{Pos2, Rect, RichText, Stroke, Ui, Vec2};

use crate::ui::theme::palette;

use super::paint::{draw_keyboard_focus, draw_recessed_well, paint_bevel_surface};

/// Glatter Silber-Hardware-Fader (0..100 % für Monomix).
pub fn hardware_fader(ui: &mut Ui, label: &str, value: &mut f32) {
    hardware_fader_custom(ui, label, value, 0.0..=1.0, 88.0, true);
}

#[allow(dead_code)]
pub fn hardware_db_fader(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    height: f32,
) {
    hardware_fader_custom(ui, label, value, range, height, false);
}

fn hardware_fader_custom(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    track_h: f32,
    percent: bool,
) {
    let p = palette(ui.ctx());
    let (min, max) = (*range.start(), *range.end());
    let span = (max - min).max(1e-5);
    ui.push_id(label, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(72.0, 0.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let name = ui.label(RichText::new(label).size(11.5).color(p.text_secondary));
                let (rect, mut response) =
                    ui.allocate_exact_size(Vec2::new(50.0, track_h), egui::Sense::click_and_drag());
                let enabled = ui.is_enabled();

                let cap_h = 18.0;
                let travel_top = rect.min.y + cap_h * 0.5 + 2.0;
                let travel_bot = rect.max.y - cap_h * 0.5 - 2.0;
                let travel_span = (travel_bot - travel_top).max(1.0);

                if enabled && (response.dragged() || response.clicked()) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let frac = ((travel_bot - pos.y) / travel_span).clamp(0.0, 1.0);
                        *value = (min + frac * span).clamp(min, max);
                        response.mark_changed();
                    }
                }
                if enabled && response.has_focus() {
                    let keys = ui.input(|input| {
                        i32::from(
                            input.key_pressed(egui::Key::ArrowUp)
                                || input.key_pressed(egui::Key::ArrowRight),
                        ) - i32::from(
                            input.key_pressed(egui::Key::ArrowDown)
                                || input.key_pressed(egui::Key::ArrowLeft),
                        )
                    });
                    if keys != 0 {
                        *value = (*value + keys as f32 * span * 0.02).clamp(min, max);
                        response.mark_changed();
                    }
                }

                response.widget_info(|| {
                    egui::WidgetInfo::slider(
                        enabled,
                        f64::from(if percent { *value * 100.0 } else { *value }),
                        label,
                    )
                });

                let painter = ui.painter();
                let cx = rect.center().x;

                for i in 0..=6 {
                    let frac = i as f32 / 6.0;
                    let y = travel_bot - frac * travel_span;
                    let is_zero = !percent && (min + frac * span).abs() < 0.5;
                    let len = if is_zero || (percent && i == 3) {
                        7.0
                    } else {
                        4.0
                    };
                    let col = if is_zero { p.accent_top } else { p.border_mid };
                    painter.line_segment(
                        [Pos2::new(cx - 11.0 - len, y), Pos2::new(cx - 11.0, y)],
                        Stroke::new(1.0, col),
                    );
                    painter.line_segment(
                        [Pos2::new(cx + 11.0, y), Pos2::new(cx + 11.0 + len, y)],
                        Stroke::new(1.0, col),
                    );
                }

                let slot_rect = Rect::from_min_max(
                    Pos2::new(cx - 3.0, travel_top - 4.0),
                    Pos2::new(cx + 3.0, travel_bot + 4.0),
                );
                draw_recessed_well(painter, slot_rect, 3.0, p.bg_inset);

                let norm = ((*value - min) / span).clamp(0.0, 1.0);
                let knob_y = travel_bot - norm * travel_span;
                let fill_rect = Rect::from_min_max(
                    Pos2::new(cx - 2.0, knob_y),
                    Pos2::new(cx + 2.0, travel_bot + 2.0),
                );
                painter.rect_filled(fill_rect, 2.0, p.accent_top);

                let cap_rect =
                    Rect::from_center_size(Pos2::new(cx, knob_y), Vec2::new(28.0, cap_h));
                paint_bevel_surface(
                    painter,
                    cap_rect,
                    5.0,
                    p.knob_top,
                    p.knob_bottom,
                    false,
                    Some(p.border_mid),
                );
                painter.line_segment(
                    [
                        Pos2::new(cap_rect.min.x + 5.0, knob_y),
                        Pos2::new(cap_rect.max.x - 5.0, knob_y),
                    ],
                    Stroke::new(1.8, p.accent_top),
                );

                draw_keyboard_focus(ui, &response);
                response.on_hover_cursor(egui::CursorIcon::ResizeVertical);

                if percent {
                    ui.add_sized(
                        [68.0, 22.0],
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(0.005)
                            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
                            .custom_parser(|text| {
                                text.trim()
                                    .trim_end_matches('%')
                                    .trim()
                                    .parse::<f64>()
                                    .ok()
                                    .map(|v| v / 100.0)
                            }),
                    )
                    .labelled_by(name.id)
                    .on_hover_text(label);
                } else {
                    ui.add_sized(
                        [68.0, 22.0],
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(0.1)
                            .max_decimals(1)
                            .suffix(" dB"),
                    )
                    .labelled_by(name.id)
                    .on_hover_text(label);
                }
            },
        );
    });
}

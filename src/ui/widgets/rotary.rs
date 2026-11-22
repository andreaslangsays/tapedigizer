use std::ops::RangeInclusive;

use egui::{Color32, Pos2, Rect, RichText, Shape, Stroke, Ui, Vec2};

use crate::ui::theme::{palette, Palette};

use super::paint::{paint_bevel_surface, paint_rounded_vertical_gradient};

#[derive(Clone, Copy)]
pub enum RotaryAccent {
    Primary,
    Signal,
    Amber,
    Limit,
}

impl RotaryAccent {
    fn color(self, palette: Palette) -> Color32 {
        match self {
            Self::Primary => palette.accent_top,
            Self::Signal => palette.teal,
            Self::Amber => palette.amber,
            Self::Limit => palette.red,
        }
    }
}

#[derive(Clone, Copy)]
pub enum RotarySize {
    Dense,
    Compact,
    Eq,
    Narrow,
    Medium,
    Standard,
    Large,
}

impl RotarySize {
    fn diameter(self) -> f32 {
        match self {
            Self::Dense => 34.0,
            Self::Compact => 36.0,
            Self::Eq => 38.0,
            Self::Narrow => 40.0,
            Self::Medium => 42.0,
            Self::Standard => 46.0,
            Self::Large => 50.0,
        }
    }
}

pub struct RotarySpec<'a> {
    pub label: &'a str,
    pub range: RangeInclusive<f32>,
    pub unit: &'a str,
    pub accent: RotaryAccent,
    pub size: RotarySize,
}

impl<'a> RotarySpec<'a> {
    pub fn new(
        label: &'a str,
        range: RangeInclusive<f32>,
        unit: &'a str,
        accent: RotaryAccent,
        size: RotarySize,
    ) -> Self {
        Self {
            label,
            range,
            unit,
            accent,
            size,
        }
    }
}

pub fn rotary(ui: &mut Ui, value: &mut f32, spec: RotarySpec<'_>) {
    let RotarySpec {
        label,
        range,
        unit,
        accent,
        size,
    } = spec;
    let p = palette(ui.ctx());
    let arc_color = accent.color(p);
    let diameter = size.diameter();
    ui.push_id(label, |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let name = ui.label(RichText::new(label).size(10.8).color(p.text_secondary));
            let (min, max) = (*range.start(), *range.end());
            let span = (max - min).max(1e-6);
            let use_log = min > 0.0 && (max / min) >= 20.0;
            let to_norm = |v: f32| -> f32 {
                if use_log {
                    ((v.clamp(min, max) / min).ln() / (max / min).ln()).clamp(0.0, 1.0)
                } else {
                    ((v - min) / span).clamp(0.0, 1.0)
                }
            };
            let from_norm = |u: f32| -> f32 {
                let u = u.clamp(0.0, 1.0);
                if use_log {
                    (min * (max / min).powf(u)).clamp(min, max)
                } else {
                    (min + u * span).clamp(min, max)
                }
            };
            let bipolar = min < 0.0 && max > 0.0;

            let mut response =
                ui.allocate_response(Vec2::splat(diameter), egui::Sense::click_and_drag());
            let shift = ui.input(|input| input.modifiers.shift);
            let drag_norm_per_px = if shift { 1.0 / 900.0 } else { 1.0 / 220.0 };
            let key_step_norm = if shift { 0.0012 } else { 0.0065 };
            let drag_position_id = response.id.with("rotary_drag_position");
            let mut next_value = None;

            if ui.is_enabled() {
                let dragging = response.is_pointer_button_down_on()
                    || response.dragged_by(egui::PointerButton::Primary);
                let finishing = response.drag_stopped_by(egui::PointerButton::Primary);
                if dragging || finishing {
                    if let Some(pos) = ui.ctx().pointer_latest_pos() {
                        let previous_y = ui
                            .data(|data| data.get_temp::<f32>(drag_position_id))
                            .or_else(|| {
                                ui.input(|input| input.pointer.press_origin().map(|pos| pos.y))
                            })
                            .unwrap_or(pos.y);
                        let change = (previous_y - pos.y) * drag_norm_per_px;
                        if change != 0.0 {
                            next_value = Some(from_norm(to_norm(*value) + change));
                        }
                        ui.data_mut(|data| data.insert_temp(drag_position_id, pos.y));
                    }
                }
                if !dragging {
                    ui.data_mut(|data| data.remove::<f32>(drag_position_id));
                }
                if response.double_clicked() && bipolar {
                    next_value = Some(0.0);
                }

                // Mausrad / Trackpad-Geste direkt über dem Poti
                if response.hovered() && !dragging && !finishing {
                    let scroll_y = ui.input(|input| input.raw_scroll_delta.y);
                    if scroll_y.abs() > 0.1 {
                        let scroll_sens = if shift { 0.0005 } else { 0.0025 };
                        next_value = Some(from_norm(to_norm(*value) + scroll_y * scroll_sens));
                    }
                }
            }

            let mut keys = if response.has_focus() {
                ui.input(|input| {
                    i32::from(
                        input.key_pressed(egui::Key::ArrowUp)
                            || input.key_pressed(egui::Key::ArrowRight),
                    ) - i32::from(
                        input.key_pressed(egui::Key::ArrowDown)
                            || input.key_pressed(egui::Key::ArrowLeft),
                    )
                })
            } else {
                0
            };
            use egui::accesskit::{Action, ActionData};
            let requested_value = ui.input(|input| {
                keys += input.num_accesskit_action_requests(response.id, Action::Increment) as i32;
                keys -= input.num_accesskit_action_requests(response.id, Action::Decrement) as i32;
                input
                    .accesskit_action_requests(response.id, Action::SetValue)
                    .filter_map(|request| match request.data {
                        Some(ActionData::NumericValue(value)) => Some(value as f32),
                        Some(ActionData::Value(ref value)) => value.trim().parse().ok(),
                        _ => None,
                    })
                    .filter(|value: &f32| value.is_finite())
                    .last()
            });
            if ui.is_enabled() && (next_value.is_some() || keys != 0 || requested_value.is_some()) {
                let next = requested_value
                    .or(next_value)
                    .unwrap_or_else(|| from_norm(to_norm(*value) + keys as f32 * key_step_norm));
                *value = next.clamp(min, max);
                response.mark_changed();
            }
            response.widget_info(|| {
                egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*value), label)
            });
            ui.ctx().accesskit_node_builder(response.id, |builder| {
                builder.set_min_numeric_value(f64::from(min));
                builder.set_max_numeric_value(f64::from(max));
                if ui.is_enabled() {
                    builder.add_action(Action::Increment);
                    builder.add_action(Action::Decrement);
                    builder.add_action(Action::SetValue);
                }
            });

            let zero = if bipolar { to_norm(0.0) } else { 0.0 };
            paint_rotary(ui.painter(), &response, p, arc_color, to_norm(*value), zero);
            let cursor = if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            };
            let tip = if bipolar {
                "Hoch/Runter ziehen oder Mausrad · Shift = Fein · Doppelklick = 0"
            } else {
                "Hoch/Runter ziehen oder Mausrad · Shift = Fein"
            };
            response.on_hover_cursor(cursor).on_hover_text(tip);
            ui.add_sized(
                [66.0, 20.0],
                egui::DragValue::new(value)
                    .range(range)
                    .speed(span * 0.001)
                    .max_decimals(2)
                    .suffix(unit),
            )
            .labelled_by(name.id)
            .on_hover_text(label);
        });
    });
}

fn paint_rotary(
    painter: &egui::Painter,
    response: &egui::Response,
    p: Palette,
    accent: Color32,
    fraction: f32,
    zero: f32,
) {
    let scale = response.rect.width() / 56.0;
    let center = response.rect.center();
    let at = |unit: f32, radius: f32| {
        let angle = (unit - 0.5) * std::f32::consts::PI * 1.5;
        center + Vec2::new(angle.sin(), -angle.cos()) * radius
    };

    // Glatter 270° Hintergrund-Bogen (wie in den Stitch-Panels)
    let arc_radius = 23.5 * scale;
    let bg_arc: Vec<Pos2> = (0..=32).map(|i| at(i as f32 / 32.0, arc_radius)).collect();
    painter.add(Shape::line(
        bg_arc,
        Stroke::new((3.0 * scale).max(2.0), p.border_subtle),
    ));

    // Glatter farbiger Wert-Bogen (von 0 bis fraction, oder bipolar von Mitte bis fraction)
    let (arc_start, arc_end) = (zero.min(fraction), zero.max(fraction));
    if arc_end - arc_start > 0.005 {
        let segs = (((arc_end - arc_start) * 36.0).ceil() as usize).max(2);
        let val_arc: Vec<Pos2> = (0..=segs)
            .map(|i| {
                let u = arc_start + (arc_end - arc_start) * (i as f32 / segs as f32);
                at(u, arc_radius)
            })
            .collect();
        painter.add(Shape::line(
            val_arc,
            Stroke::new((3.2 * scale).max(2.2), accent),
        ));
    }

    // Weicher Schatten & glatte Silber-Aluminium-Kappe
    painter.circle_filled(
        center + Vec2::new(0.0, 1.2 * scale),
        17.0 * scale,
        Color32::from_rgba_unmultiplied(0, 0, 0, if p.is_dark { 115 } else { 30 }),
    );
    let knob_diam = 33.0 * scale;
    let knob_rect = Rect::from_center_size(center, Vec2::splat(knob_diam));
    let active_ring = if response.dragged() {
        Some(Color32::from_rgba_unmultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            210,
        ))
    } else if response.hovered() {
        Some(Color32::from_rgba_unmultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            120,
        ))
    } else {
        Some(p.border_mid)
    };
    paint_bevel_surface(
        painter,
        knob_rect,
        knob_diam * 0.5,
        p.knob_top,
        p.knob_bottom,
        response.dragged(),
        active_ring,
    );

    let cap_diam = 23.0 * scale;
    let cap_rect = Rect::from_center_size(center, Vec2::splat(cap_diam));
    paint_rounded_vertical_gradient(
        painter,
        cap_rect,
        cap_diam * 0.5,
        p.knob_cap_top,
        p.knob_cap_bottom,
    );

    // Klarer farbiger Zeigerstrich
    painter.line_segment(
        [at(fraction, 5.5 * scale), at(fraction, 14.2 * scale)],
        Stroke::new((2.4 * scale).max(1.8), accent),
    );
    if response.has_focus() {
        painter.circle_stroke(center, 27.5 * scale, Stroke::new(2.0, p.focus));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::{Action, ActionData, ActionRequest, Role};

    #[test]
    fn rotary_accepts_accessibility_values_and_keyboard_steps() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut value = 0.0;
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rotary(
                    ui,
                    &mut value,
                    RotarySpec::new(
                        "Song gain",
                        -24.0..=12.0,
                        " dB",
                        RotaryAccent::Primary,
                        RotarySize::Standard,
                    ),
                );
            });
        };
        let output = ctx.run(egui::RawInput::default(), &mut draw);
        let tree = output.platform_output.accesskit_update.unwrap();
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Slider && node.name() == Some("Song gain"))
            .expect("the custom rotary exposes a named slider");
        let target = *target;
        let output = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    target,
                    action: Action::SetValue,
                    data: Some(ActionData::Value("3.5".into())),
                })],
                ..Default::default()
            },
            &mut draw,
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| *id == target)
                .unwrap()
                .1
                .numeric_value(),
            Some(3.5)
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::AccessKitActionRequest(ActionRequest {
                        target,
                        action: Action::Focus,
                        data: None,
                    }),
                    egui::Event::Key {
                        key: egui::Key::ArrowRight,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            &mut draw,
        );
        assert!(value > 3.5 && value < 4.0);
    }

    #[test]
    fn rotary_responds_to_vertical_pointer_drag() {
        let bipolar = drag_up(-24.0..=12.0, 0.0).0;
        assert!((bipolar - 6.55).abs() < 0.03);

        let logarithmic = drag_up(5.0..=500.0, 50.0).0;
        assert!((logarithmic - 115.51).abs() < 0.1);
    }

    fn drag_up(range: RangeInclusive<f32>, mut value: f32) -> (f32, f32) {
        let ctx = egui::Context::default();
        let screen = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 300.0)));
        let knob_center = std::cell::Cell::new(Pos2::ZERO);
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(Vec2::new(90.0, 110.0), |ui| {
                    rotary(
                        ui,
                        &mut value,
                        RotarySpec::new(
                            "Test parameter",
                            range.clone(),
                            "",
                            RotaryAccent::Primary,
                            RotarySize::Standard,
                        ),
                    );
                    knob_center.set(ui.min_rect().center());
                });
            });
        };
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.0),
                ..Default::default()
            },
            &mut draw,
        );
        let center = knob_center.get();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.05),
                events: vec![egui::Event::PointerMoved(center)],
                ..Default::default()
            },
            &mut draw,
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.1),
                events: vec![egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            &mut draw,
        );
        let dragged_pos = center - Vec2::new(0.0, 40.0);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.2),
                events: vec![egui::Event::PointerMoved(dragged_pos)],
                ..Default::default()
            },
            &mut draw,
        );
        drop(draw);
        let raised = value;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.3),
                events: vec![egui::Event::PointerMoved(center)],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| {
                        ui.allocate_ui(Vec2::new(90.0, 110.0), |ui| {
                            rotary(
                                ui,
                                &mut value,
                                RotarySpec::new(
                                    "Test parameter",
                                    range.clone(),
                                    "",
                                    RotaryAccent::Primary,
                                    RotarySize::Standard,
                                ),
                            );
                        });
                    })
                    .inner
            },
        );
        (raised, value)
    }

    #[test]
    fn rotary_returns_to_start_value_when_pointer_returns_to_press_point() {
        let (_, returned) = drag_up(-24.0..=12.0, 0.0);
        assert!(returned.abs() < 0.01, "got {returned}");
    }
}

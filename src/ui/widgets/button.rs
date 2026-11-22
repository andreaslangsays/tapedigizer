use egui::{Button, Color32, Pos2, Rect, RichText, Stroke, Ui, Vec2, WidgetText};

use crate::ui::theme::{palette, HEIGHT_COMPACT, HEIGHT_CONTROL, HEIGHT_PRIMARY, RADIUS_CONTROL};

use super::paint::{draw_keyboard_focus, lerp_color, paint_bevel_surface};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BevelStyle {
    Neutral,
    Primary,
    Record,
    Engaged,
    Compact,
    CompactEngaged,
}

pub fn bevel_button(ui: &mut Ui, label: &str, style: BevelStyle) -> egui::Response {
    bevel_button_sized(ui, label, 0.0, style)
}

pub fn bevel_button_sized(
    ui: &mut Ui,
    label: &str,
    min_width: f32,
    style: BevelStyle,
) -> egui::Response {
    let p = palette(ui.ctx());
    let height = match style {
        BevelStyle::Primary | BevelStyle::Record => HEIGHT_PRIMARY,
        BevelStyle::Compact | BevelStyle::CompactEngaged => HEIGHT_COMPACT,
        BevelStyle::Neutral | BevelStyle::Engaged => HEIGHT_CONTROL,
    };
    let is_compact = matches!(style, BevelStyle::Compact | BevelStyle::CompactEngaged);
    let radius = if is_compact { 5.5 } else { RADIUS_CONTROL };
    let font_size = if is_compact { 11.2 } else { 12.0 };
    let pad_x = if is_compact { 5.0 } else { 10.0 };

    let font = egui::FontId::proportional(font_size);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, p.text_primary);
    let width = (galley.size().x + pad_x * 2.0).max(min_width);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());

    let enabled = ui.is_enabled();
    let pressed = enabled && response.is_pointer_button_down_on();
    let hovered = enabled && response.hovered();

    let (top, bottom, text_col, stroke) = match style {
        BevelStyle::Primary | BevelStyle::Engaged | BevelStyle::CompactEngaged => {
            if !enabled {
                (p.bg_card, p.bg_card, p.text_muted, None)
            } else if pressed {
                (p.accent_bottom, p.accent_top, p.text_on_accent, None)
            } else if hovered {
                (
                    lerp_color(p.accent_top, Color32::WHITE, 0.12),
                    p.accent_top,
                    p.text_on_accent,
                    None,
                )
            } else {
                (p.accent_top, p.accent_bottom, p.text_on_accent, None)
            }
        }
        BevelStyle::Record => {
            if !enabled {
                (p.red_dim, p.red_dim, p.text_muted, None)
            } else if pressed {
                (p.red_bottom, p.red, Color32::WHITE, None)
            } else if hovered {
                (
                    lerp_color(p.red, Color32::WHITE, 0.10),
                    p.red,
                    Color32::WHITE,
                    None,
                )
            } else {
                (p.red, p.red_bottom, Color32::WHITE, None)
            }
        }
        BevelStyle::Neutral | BevelStyle::Compact => {
            if !enabled {
                (p.bg_card, p.bg_card, p.text_muted, Some(p.border_subtle))
            } else if pressed {
                (
                    p.btn_pressed_top,
                    p.btn_pressed_bottom,
                    p.text_primary,
                    Some(p.border_mid),
                )
            } else if hovered {
                (
                    p.btn_hover_top,
                    p.btn_hover_bottom,
                    p.text_primary,
                    Some(p.border_mid),
                )
            } else {
                (
                    p.btn_top,
                    p.btn_bottom,
                    p.text_primary,
                    Some(p.border_subtle),
                )
            }
        }
    };

    paint_bevel_surface(ui.painter(), rect, radius, top, bottom, pressed, stroke);

    let text_pos = Pos2::new(
        rect.center().x,
        rect.center().y + if pressed { 0.5 } else { 0.0 },
    );
    ui.painter().text(
        text_pos,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        text_col,
    );

    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    draw_keyboard_focus(ui, &response);
    response
}

/// Symmetrisches Pill-Segmented-Control (Silverface & Obsidian, aktives Segment in Kupfer-Amber).
pub fn segmented_control<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    current: &mut T,
    items: &[(T, &str, &str)],
    height: f32,
) -> bool {
    let p = palette(ui.ctx());
    let mut changed = false;
    let enabled = ui.is_enabled();
    let pad = 2.5;
    let font = egui::FontId::proportional(if height >= 26.0 { 12.0 } else { 11.4 });
    let track_r = height * 0.5;
    let thumb_r = (height - pad * 2.0) * 0.5;

    let max_seg_w = items
        .iter()
        .map(|(_, label, _)| {
            let w = ui
                .painter()
                .layout_no_wrap((*label).to_string(), font.clone(), p.text_primary)
                .size()
                .x;
            (w + 20.0).max(if height >= 26.0 { 72.0 } else { 56.0 })
        })
        .fold(0.0_f32, f32::max);
    let total_width = max_seg_w * items.len() as f32 + pad * 2.0;

    ui.push_id(id_salt, |ui| {
        let (track_rect, _) =
            ui.allocate_exact_size(Vec2::new(total_width, height), egui::Sense::hover());
        ui.painter().rect_filled(track_rect, track_r, p.bg_inset);
        ui.painter().rect_stroke(
            track_rect.shrink(0.5),
            track_r,
            Stroke::new(0.75, p.border_subtle),
        );

        let mut x = track_rect.min.x + pad;
        for (idx, &(value, label, shortcut)) in items.iter().enumerate() {
            let seg_w = max_seg_w;
            let seg_rect = Rect::from_min_size(
                Pos2::new(x, track_rect.min.y + pad),
                Vec2::new(seg_w, height - pad * 2.0),
            );
            x += seg_w;

            let seg_id = ui.make_persistent_id(idx);
            let resp = ui.interact(seg_rect, seg_id, egui::Sense::click());
            let selected = *current == value;
            let pressed = enabled && resp.is_pointer_button_down_on();
            let hovered = enabled && resp.hovered();

            if selected {
                let (top, bot) = if pressed {
                    (p.accent_bottom, p.accent_top)
                } else if hovered {
                    (lerp_color(p.accent_top, Color32::WHITE, 0.1), p.accent_top)
                } else {
                    (p.accent_top, p.accent_bottom)
                };
                paint_bevel_surface(ui.painter(), seg_rect, thumb_r, top, bot, pressed, None);
            } else if hovered {
                let hover_fill = if p.is_dark {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, 12)
                };
                ui.painter().rect_filled(seg_rect, thumb_r, hover_fill);
            }

            let text_col = if !enabled {
                p.text_muted
            } else if selected {
                p.text_on_accent
            } else if hovered {
                p.text_primary
            } else {
                p.text_secondary
            };
            ui.painter().text(
                seg_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                text_col,
            );

            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, label)
            });
            draw_keyboard_focus(ui, &resp);
            let resp = if shortcut.is_empty() {
                resp
            } else {
                resp.on_hover_text(shortcut)
            };
            if enabled && resp.clicked() && !selected {
                *current = value;
                changed = true;
            }
        }
    });
    changed
}

pub fn primary_button(ui: &Ui, label: impl Into<egui::WidgetText>) -> Button<'static> {
    let p = palette(ui.ctx());
    let label = label.into().strong().color(p.text_on_accent);
    Button::new(label)
        .fill(p.accent_top)
        .stroke(Stroke::NONE)
        .rounding(RADIUS_CONTROL)
        .min_size(Vec2::new(84.0, HEIGHT_PRIMARY))
}

pub fn mode_button(ui: &Ui, label: impl Into<egui::WidgetText>, selected: bool) -> Button<'static> {
    let p = palette(ui.ctx());
    let label = label.into();
    let button = Button::new(if selected {
        label.strong().color(p.text_on_accent)
    } else {
        label.color(p.text_secondary)
    })
    .selected(selected)
    .rounding(RADIUS_CONTROL)
    .min_size(Vec2::new(0.0, HEIGHT_CONTROL));
    if selected {
        button.fill(p.accent_top).stroke(Stroke::NONE)
    } else {
        button
            .fill(p.btn_bottom)
            .stroke(Stroke::new(0.75, p.border_subtle))
    }
}

pub fn transport_button<'a>(ui: &Ui, label: &'a str, engaged: bool) -> Button<'a> {
    let p = palette(ui.ctx());
    Button::new(RichText::new(label).strong().color(if engaged {
        p.text_on_accent
    } else {
        p.text_primary
    }))
    .fill(if engaged { p.accent_top } else { p.btn_bottom })
    .stroke(if engaged {
        Stroke::NONE
    } else {
        Stroke::new(0.75, p.border_subtle)
    })
    .rounding(RADIUS_CONTROL)
    .min_size(Vec2::new(64.0, HEIGHT_PRIMARY))
}

pub fn action_button(label: impl Into<WidgetText>) -> Button<'static> {
    Button::new(label).rounding(RADIUS_CONTROL)
}

pub fn compact_button(label: impl Into<WidgetText>) -> Button<'static> {
    action_button(label).small()
}

pub fn compact_primary_button(ui: &Ui, label: impl Into<WidgetText>) -> Button<'static> {
    let p = palette(ui.ctx());
    compact_button(label.into().strong().color(p.text_on_bright_accent))
        .fill(p.accent_top)
        .rounding(RADIUS_CONTROL)
}

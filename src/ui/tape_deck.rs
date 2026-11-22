use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

use super::theme::{palette, RADIUS_WELL};
use super::widgets::paint::{draw_recessed_well, paint_bevel_surface};

/// Animiertes Tape-Transport-Deck (Silverface Light & Obsidian Dark).
pub fn draw_tape_transport_deck(
    ui: &mut Ui,
    deck_h: f32,
    elapsed_secs: f64,
    nominal_total_secs: f64,
    is_running: bool,
    is_recording: bool,
    medium_label: &str,
) {
    let p = palette(ui.ctx());
    let desired = Vec2::new(ui.available_width(), deck_h);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();

    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let progress = if nominal_total_secs > 1.0 {
        (elapsed_secs / nominal_total_secs).clamp(0.0, 1.0) as f32
    } else {
        ((elapsed_secs % 1800.0) / 1800.0) as f32
    };

    let scale = (deck_h / 118.0).clamp(0.7, 1.15);
    let reel_y = rect.min.y + (38.0 * scale).max((deck_h - 26.0) * 0.45);
    let reel_offset = rect.width().min(360.0) * 0.23;
    let left_c = Pos2::new(rect.center().x - reel_offset, reel_y);
    let right_c = Pos2::new(rect.center().x + reel_offset, reel_y);
    let max_r = 26.0 * scale;
    let hub_r = 10.5 * scale;

    let left_pack_r = hub_r + (max_r - hub_r - 2.0) * (1.0 - progress * 0.75);
    let right_pack_r = hub_r + (max_r - hub_r - 2.0) * (0.25 + progress * 0.75);
    let angle = if is_running {
        (elapsed_secs * 2.2) as f32
    } else {
        0.4
    };

    let housing_col = if p.is_dark {
        Color32::from_rgb(20, 22, 27)
    } else {
        Color32::from_rgb(42, 46, 54)
    };
    let pack_col = if p.is_dark {
        Color32::from_rgb(48, 36, 26)
    } else {
        Color32::from_rgb(56, 50, 44)
    };
    let hub_col = if p.is_dark {
        Color32::from_rgb(54, 58, 68)
    } else {
        Color32::from_rgb(236, 239, 245)
    };

    for (center, pack_r, dir) in [
        (left_c, left_pack_r, 1.0_f32),
        (right_c, right_pack_r, 1.05_f32),
    ] {
        painter.circle_filled(center, max_r, housing_col);
        painter.circle_filled(center, pack_r, pack_col);
        painter.circle_filled(center, hub_r, hub_col);
        for spoke in 0..3 {
            let a = angle * dir + spoke as f32 * (std::f32::consts::TAU / 3.0);
            let p0 = center + Vec2::new(a.cos(), a.sin()) * 3.5;
            let p1 = center + Vec2::new(a.cos(), a.sin()) * (hub_r - 1.5);
            painter.line_segment([p0, p1], Stroke::new(2.0, housing_col));
        }
        painter.circle_filled(center, 2.6, p.accent_top);
    }

    painter.line_segment(
        [
            Pos2::new(left_c.x, left_c.y + left_pack_r - 1.0),
            Pos2::new(right_c.x, right_c.y + right_pack_r - 1.0),
        ],
        Stroke::new(1.2, Color32::from_rgb(104, 86, 66)),
    );

    let head_center = Pos2::new(rect.center().x, reel_y - 2.0);
    let head_rect = Rect::from_center_size(head_center, Vec2::new(28.0, 14.0));
    painter.rect_filled(head_rect, 3.0, p.bg_panel);
    let head_col = if is_recording {
        p.red
    } else if is_running {
        p.teal
    } else {
        p.text_muted
    };
    painter.line_segment(
        [
            Pos2::new(head_rect.min.x + 5.0, head_rect.min.y + 3.0),
            Pos2::new(head_rect.max.x - 5.0, head_rect.min.y + 3.0),
        ],
        Stroke::new(2.0, head_col),
    );
    painter.text(
        Pos2::new(head_center.x, head_rect.max.y + 7.0),
        egui::Align2::CENTER_CENTER,
        "HEAD",
        egui::FontId::monospace(8.5),
        p.text_muted,
    );

    let counter_val = (elapsed_secs as u64) % 10_000;
    let digits = format!("{counter_val:04}");
    let digit_w = 18.0;
    let digit_h = 22.0;
    let start_x = rect.min.x + 12.0;
    let digit_y = rect.max.y - digit_h - 9.0;
    for (i, ch) in digits.chars().enumerate() {
        let dr = Rect::from_min_size(
            Pos2::new(start_x + i as f32 * (digit_w + 3.0), digit_y),
            Vec2::new(digit_w, digit_h),
        );
        paint_bevel_surface(
            painter,
            dr,
            4.0,
            p.btn_top,
            p.btn_bottom,
            false,
            Some(p.border_subtle),
        );
        painter.text(
            dr.center(),
            egui::Align2::CENTER_CENTER,
            ch.to_string(),
            egui::FontId::monospace(13.0),
            p.amber,
        );
    }

    painter.text(
        Pos2::new(rect.max.x - 12.0, digit_y + digit_h * 0.5),
        egui::Align2::RIGHT_CENTER,
        medium_label,
        egui::FontId::monospace(10.0),
        p.teal,
    );
}

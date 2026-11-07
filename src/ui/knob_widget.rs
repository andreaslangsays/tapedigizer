use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use std::f32::consts::PI;
use crate::ui::theme::{
    draw_recessed_well, BG_CARD, BG_INSET, BORDER_DARK, BORDER_LIGHT, LED_AMBER,
    LED_GREEN, LED_RED, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY,
};

pub fn render_gain_knob_card(
    ui: &mut Ui,
    title: &str,
    current_peak_db: f32,
    adjustment_db: f32, // Positive = boost, Negative = cut, 0 = optimal
    is_clipping: bool,
) {
    let desired_size = Vec2::new(ui.available_width().max(300.0), 132.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    let painter = ui.painter();

    // Studio Rack Plate
    painter.rect_filled(rect, 8.0, BG_CARD);
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, BORDER_DARK));

    // Simulated Rack Screws in the 4 corners
    draw_rack_screw(painter, Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0));
    draw_rack_screw(painter, Pos2::new(rect.max.x - 8.0, rect.min.y + 8.0));
    draw_rack_screw(painter, Pos2::new(rect.min.x + 8.0, rect.max.y - 8.0));
    draw_rack_screw(painter, Pos2::new(rect.max.x - 8.0, rect.max.y - 8.0));

    // Left Side: Simulated Analog Knob
    let knob_center = Pos2::new(rect.min.x + 80.0, rect.center().y);
    let knob_radius = 34.0;

    // Outer Calibrated Arc (-135 deg to +135 deg)
    let start_angle = -135.0 * PI / 180.0;
    let end_angle = 135.0 * PI / 180.0;

    let arc_radius = knob_radius + 12.0;

    // Draw dial tick marks
    for i in 0..=10 {
        let frac = i as f32 / 10.0;
        let angle = start_angle + frac * (end_angle - start_angle);
        let cos = angle.cos();
        let sin = angle.sin();

        let inner_pt = Pos2::new(knob_center.x + (arc_radius - 4.0) * sin, knob_center.y - (arc_radius - 4.0) * cos);
        let outer_pt = Pos2::new(knob_center.x + arc_radius * sin, knob_center.y - arc_radius * cos);

        painter.line_segment([inner_pt, outer_pt], Stroke::new(1.5, BORDER_LIGHT));
    }

    // Machined Aluminum Knob Body
    painter.circle_filled(knob_center, knob_radius, Color32::from_rgb(52, 54, 62));
    painter.circle_stroke(knob_center, knob_radius, Stroke::new(2.0, BORDER_DARK));

    // Metallic Knurled Rim highlight
    painter.circle_stroke(knob_center, knob_radius - 2.0, Stroke::new(1.0, Color32::from_rgb(72, 75, 86)));
    painter.circle_filled(knob_center, knob_radius - 4.0, Color32::from_rgb(44, 46, 53));

    // Knob Pointer Angle based on adjustment
    // Neutral center is 0 deg.
    let target_angle = if is_clipping {
        -45.0 * PI / 180.0
    } else if adjustment_db > 0.0 {
        (adjustment_db.min(12.0) / 12.0 * 90.0) * PI / 180.0
    } else {
        0.0
    };

    let notch_outer = Pos2::new(
        knob_center.x + (knob_radius - 4.0) * target_angle.sin(),
        knob_center.y - (knob_radius - 4.0) * target_angle.cos(),
    );
    let notch_inner = Pos2::new(
        knob_center.x + (knob_radius - 16.0) * target_angle.sin(),
        knob_center.y - (knob_radius - 16.0) * target_angle.cos(),
    );

    let pointer_col = if is_clipping {
        LED_RED
    } else if adjustment_db > 1.0 {
        LED_AMBER
    } else {
        LED_GREEN
    };
    painter.line_segment([notch_inner, notch_outer], Stroke::new(3.0, pointer_col));

    // Right Side: Clear Hardware Action Card
    let info_left = rect.min.x + 160.0;
    let info_rect = Rect::from_min_max(
        Pos2::new(info_left, rect.min.y + 16.0),
        Pos2::new(rect.max.x - 20.0, rect.max.y - 16.0),
    );

    draw_recessed_well(painter, info_rect, 6.0, BG_INSET);

    // Title badge
    painter.text(
        Pos2::new(info_rect.min.x + 16.0, info_rect.min.y + 12.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(12.0),
        TEXT_MUTED,
    );

    // Big Action Display
    let (action_title, val_str, val_col) = if is_clipping {
        ("GAIN ZURÜCKDREHEN", format!("ca. {:.0} dB absenken", adjustment_db.abs().max(2.0)), LED_RED)
    } else if adjustment_db > 1.0 {
        ("GAIN AUFDREHEN", format!("ca. +{:.0} dB weiter aufdrehen", adjustment_db), LED_AMBER)
    } else {
        ("GAIN OPTIMAL", "Einstellung beibehalten (Perfekt)".into(), LED_GREEN)
    };

    painter.text(
        Pos2::new(info_rect.min.x + 16.0, info_rect.min.y + 32.0),
        egui::Align2::LEFT_TOP,
        action_title,
        egui::FontId::proportional(15.0),
        val_col,
    );

    painter.text(
        Pos2::new(info_rect.min.x + 16.0, info_rect.min.y + 54.0),
        egui::Align2::LEFT_TOP,
        format!("Am Zoom L-12: {}", val_str),
        egui::FontId::proportional(14.0),
        TEXT_PRIMARY,
    );

    let subtitle = format!(
        "Maximaler Peak lag bei {:.1} dBFS  (Zielbereich: -1.5 dBFS)",
        current_peak_db
    );
    painter.text(
        Pos2::new(info_rect.min.x + 16.0, info_rect.min.y + 78.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::monospace(11.0),
        TEXT_SECONDARY,
    );
}

fn draw_rack_screw(painter: &egui::Painter, center: Pos2) {
    painter.circle_filled(center, 4.0, Color32::from_rgb(32, 34, 38));
    painter.circle_stroke(center, 4.0, Stroke::new(1.0, Color32::from_rgb(52, 54, 60)));
    // Screw slot
    painter.line_segment(
        [Pos2::new(center.x - 2.5, center.y - 2.5), Pos2::new(center.x + 2.5, center.y + 2.5)],
        Stroke::new(1.0, Color32::from_rgb(68, 70, 78)),
    );
}

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use crate::audio::meter::MeterLevels;
use crate::ui::theme::{
    draw_recessed_well, BG_CARD, BG_INSET, BORDER_DARK, LED_AMBER,
    LED_AMBER_DIM, LED_GREEN, LED_GREEN_DIM, LED_RED, LED_RED_DIM, TEXT_MUTED,
};

pub fn render_stereo_meter(ui: &mut Ui, meter: &MeterLevels) {
    let desired_size = Vec2::new(ui.available_width().min(740.0), 84.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    let painter = ui.painter();

    // Studio Rack Bezel
    painter.rect_filled(rect, 8.0, BG_CARD);
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, BORDER_DARK));

    let margin = 12.0;
    let label_width = 32.0;
    let clip_width = 46.0;
    let num_width = 72.0;
    let bar_height = 16.0;

    let bar_left = rect.min.x + margin + label_width;
    let bar_right = rect.max.x - margin - clip_width - num_width - 16.0;
    let bar_width = (bar_right - bar_left).max(120.0);

    // Channel Left (L)
    let y_l = rect.min.y + margin + 4.0;
    painter.text(
        Pos2::new(rect.min.x + margin, y_l + 1.0),
        egui::Align2::LEFT_TOP,
        "CH 1",
        egui::FontId::monospace(11.0),
        Color32::from_rgb(56, 189, 248),
    );
    draw_led_ladder_bar(
        painter,
        bar_left,
        y_l,
        bar_width,
        bar_height,
        meter.peak_l_db,
        meter.rms_l_db,
        meter.peak_hold_l_db,
    );

    // Digital readout L
    draw_digital_db_box(
        painter,
        Rect::from_min_size(Pos2::new(bar_right + 8.0, y_l), Vec2::new(num_width, bar_height)),
        meter.peak_l_db,
    );

    // Channel Right (R)
    let y_r = y_l + bar_height + 14.0;
    painter.text(
        Pos2::new(rect.min.x + margin, y_r + 1.0),
        egui::Align2::LEFT_TOP,
        "CH 2",
        egui::FontId::monospace(11.0),
        Color32::from_rgb(147, 130, 248),
    );
    draw_led_ladder_bar(
        painter,
        bar_left,
        y_r,
        bar_width,
        bar_height,
        meter.peak_r_db,
        meter.rms_r_db,
        meter.peak_hold_r_db,
    );

    // Digital readout R
    draw_digital_db_box(
        painter,
        Rect::from_min_size(Pos2::new(bar_right + 8.0, y_r), Vec2::new(num_width, bar_height)),
        meter.peak_r_db,
    );

    // Latching CLIP Indicator Lamp
    let clip_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - margin - clip_width, y_l + 2.0),
        Vec2::new(clip_width, bar_height * 2.0 + 10.0),
    );
    draw_clip_lamp(painter, clip_rect, meter.is_clipping);

    // Calibrated Scale Ticks & Labels
    let scale_y = y_r + bar_height + 4.0;
    let ticks = [-48, -36, -24, -18, -12, -6, -3, -1, 0];
    for &db in &ticks {
        let frac = db_to_norm_fraction(db as f32);
        let tick_x = bar_left + frac * bar_width;
        painter.line_segment(
            [Pos2::new(tick_x, y_l - 2.0), Pos2::new(tick_x, scale_y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
        );

        let label = if db == 0 { "0" } else { &format!("{}", db) };
        let col = if db >= -1 {
            LED_RED
        } else if db >= -6 {
            LED_AMBER
        } else {
            TEXT_MUTED
        };
        painter.text(
            Pos2::new(tick_x, scale_y + 2.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::monospace(9.0),
            col,
        );
    }
}

fn db_to_norm_fraction(db: f32) -> f32 {
    let min_db = -60.0_f32;
    let max_db = 0.0_f32;
    ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0)
}

fn draw_led_ladder_bar(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    peak_db: f32,
    _rms_db: f32,
    peak_hold_db: f32,
) {
    let well_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
    draw_recessed_well(painter, well_rect, 3.0, BG_INSET);

    let num_segments: usize = 36;
    let gap = 2.0;
    let seg_width = ((width - (num_segments as f32 - 1.0) * gap) / num_segments as f32).max(1.5);

    let peak_frac = db_to_norm_fraction(peak_db);
    let hold_frac = db_to_norm_fraction(peak_hold_db);

    let lit_count = (peak_frac * num_segments as f32).round() as usize;
    let hold_idx = ((hold_frac * num_segments as f32).floor() as usize).min(num_segments.saturating_sub(1));

    for i in 0..num_segments {
        let seg_x = x + (i as f32) * (seg_width + gap);
        let seg_rect = Rect::from_min_size(Pos2::new(seg_x, y + 2.0), Vec2::new(seg_width, height - 4.0));

        // Segment threshold db
        let seg_frac = (i + 1) as f32 / num_segments as f32;
        let seg_db = -60.0 + seg_frac * 60.0;

        let (lit_color, dim_color) = if seg_db >= -1.0 {
            (LED_RED, LED_RED_DIM)
        } else if seg_db >= -6.0 {
            (LED_AMBER, LED_AMBER_DIM)
        } else {
            (LED_GREEN, LED_GREEN_DIM)
        };

        let is_lit = i < lit_count;
        let is_hold = i == hold_idx && peak_hold_db > -58.0;

        let color = if is_lit {
            lit_color
        } else if is_hold {
            Color32::WHITE
        } else {
            dim_color
        };

        painter.rect_filled(seg_rect, 1.0, color);

        // Highlight glint on lit LEDs
        if is_lit || is_hold {
            painter.line_segment(
                [Pos2::new(seg_x, y + 2.5), Pos2::new(seg_x + seg_width, y + 2.5)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 90)),
            );
        }
    }
}

fn draw_digital_db_box(painter: &egui::Painter, rect: Rect, db: f32) {
    draw_recessed_well(painter, rect, 3.0, BG_INSET);

    let text = if db <= -59.5 {
        "-∞ dB".to_string()
    } else {
        format!("{:+.1} dB", db)
    };

    let col = if db >= -0.5 {
        LED_RED
    } else if db >= -6.0 {
        LED_AMBER
    } else {
        LED_GREEN
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(11.0),
        col,
    );
}

fn draw_clip_lamp(painter: &egui::Painter, rect: Rect, is_clipping: bool) {
    let (bg, border, text_col) = if is_clipping {
        (
            LED_RED,
            Color32::from_rgb(255, 180, 180),
            Color32::WHITE,
        )
    } else {
        (
            Color32::from_rgb(32, 22, 24),
            Color32::from_rgb(52, 34, 38),
            Color32::from_rgb(110, 48, 56),
        )
    };

    painter.rect_filled(rect, 4.0, bg);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, border));

    // Outer glow when clipping
    if is_clipping {
        let glow_rect = rect.expand(2.0);
        painter.rect_stroke(glow_rect, 5.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(231, 76, 60, 140)));
    }

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "CLIP",
        egui::FontId::monospace(11.0),
        text_col,
    );
}

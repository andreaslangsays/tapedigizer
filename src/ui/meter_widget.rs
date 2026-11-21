use crate::audio::meter::MeterLevels;
use crate::ui::theme::{
    draw_keyboard_focus, draw_recessed_well, paint_rounded_vertical_gradient, palette, RADIUS_WELL,
};
use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};

pub fn render_stereo_meter(
    ui: &mut Ui,
    meter: &MeterLevels,
    label: &str,
    reset_label: &str,
    clipping_label: &str,
) -> bool {
    let p = palette();
    let desired_size = Vec2::new(ui.available_width().max(260.0), 72.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    response.widget_info(|| {
        let clipping = if meter.is_clipping {
            format!(". {clipping_label}")
        } else {
            String::new()
        };
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!(
                "{label}. L {}. R {}{clipping}. {reset_label}",
                format_db(meter.peak_l_db),
                format_db(meter.peak_r_db)
            ),
        )
    });
    let painter = ui.painter();

    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let margin = 9.0;
    let label_width = 24.0;
    let clip_width = 48.0;
    let num_width = 68.0;
    let bar_height = 14.0;

    let bar_left = rect.min.x + margin + label_width;
    let bar_right = rect.max.x - margin - clip_width - num_width - 14.0;
    let bar_width = (bar_right - bar_left).max(100.0);

    // Channel Left (L)
    let y_l = rect.min.y + margin + 2.0;
    painter.text(
        Pos2::new(rect.min.x + margin, y_l + 1.0),
        egui::Align2::LEFT_TOP,
        "L",
        egui::FontId::monospace(11.0),
        p.wave_left,
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
    draw_digital_db_box(
        painter,
        Rect::from_min_size(
            Pos2::new(bar_right + 8.0, y_l),
            Vec2::new(num_width, bar_height),
        ),
        meter.peak_l_db,
    );

    // Channel Right (R)
    let y_r = y_l + bar_height + 8.0;
    painter.text(
        Pos2::new(rect.min.x + margin, y_r + 1.0),
        egui::Align2::LEFT_TOP,
        "R",
        egui::FontId::monospace(11.0),
        p.wave_right,
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
    draw_digital_db_box(
        painter,
        Rect::from_min_size(
            Pos2::new(bar_right + 8.0, y_r),
            Vec2::new(num_width, bar_height),
        ),
        meter.peak_r_db,
    );

    // Latching CLIP Indicator
    let clip_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - margin - clip_width, y_l + 1.0),
        Vec2::new(clip_width, bar_height * 2.0 + 6.0),
    );
    draw_clip_lamp(painter, clip_rect, meter.is_clipping);

    // Calibrated Scale Ticks & Labels
    let scale_y = y_r + bar_height + 4.0;
    let tick_col = if p.is_dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, 28)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 22)
    };
    let ticks = [-48, -36, -24, -18, -12, -6, 0];
    for &db in &ticks {
        let frac = db_to_norm_fraction(db as f32);
        let tick_x = bar_left + frac * bar_width;
        painter.line_segment(
            [Pos2::new(tick_x, y_l - 2.0), Pos2::new(tick_x, scale_y)],
            Stroke::new(1.0, tick_col),
        );

        let label = if db == 0 {
            "0".to_string()
        } else {
            format!("{db}")
        };
        let col = if db >= -1 {
            p.red
        } else if db >= -6 {
            p.amber
        } else {
            p.text_muted
        };
        painter.text(
            Pos2::new(tick_x, scale_y + 2.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::monospace(9.0),
            col,
        );
    }

    draw_keyboard_focus(ui, &response);
    response.on_hover_text(reset_label).clicked()
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
    let p = palette();
    let well_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
    painter.rect_filled(well_rect, 2.0, p.bg_inset);

    let num_segments: usize = 36;
    let gap = 2.0;
    let seg_width = ((width - (num_segments as f32 - 1.0) * gap) / num_segments as f32).max(1.5);

    let peak_frac = db_to_norm_fraction(peak_db);
    let hold_frac = db_to_norm_fraction(peak_hold_db);

    let lit_count = (peak_frac * num_segments as f32).round() as usize;
    let hold_idx =
        ((hold_frac * num_segments as f32).floor() as usize).min(num_segments.saturating_sub(1));

    for i in 0..num_segments {
        let seg_x = x + (i as f32) * (seg_width + gap);
        let seg_rect = Rect::from_min_size(
            Pos2::new(seg_x, y + 2.0),
            Vec2::new(seg_width, height - 4.0),
        );

        let seg_frac = (i + 1) as f32 / num_segments as f32;
        let seg_db = -60.0 + seg_frac * 60.0;

        let (lit_color, dim_color) = if seg_db >= -1.0 {
            (p.red, p.red_dim)
        } else if seg_db >= -6.0 {
            (p.amber, p.amber_dim)
        } else {
            (p.teal, p.teal_dim)
        };

        let is_lit = i < lit_count;
        let is_hold = i == hold_idx && peak_hold_db > -58.0;

        let color = if is_lit {
            lit_color
        } else if is_hold {
            if p.is_dark {
                Color32::WHITE
            } else {
                p.text_primary
            }
        } else {
            dim_color
        };

        painter.rect_filled(seg_rect, 1.5, color);
    }
}

fn format_db(db: f32) -> String {
    if db <= -59.5 {
        "-∞ dBFS".to_string()
    } else {
        format!("{db:+.1} dBFS")
    }
}

fn draw_digital_db_box(painter: &egui::Painter, rect: Rect, db: f32) {
    let p = palette();
    let text = format_db(db);
    let col = if db >= -0.5 {
        p.red
    } else if db >= -6.0 {
        p.amber
    } else {
        p.text_secondary
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(10.5),
        col,
    );
}

fn draw_clip_lamp(painter: &egui::Painter, rect: Rect, is_clipping: bool) {
    let p = palette();
    if is_clipping {
        painter.rect_filled(rect, 4.0, p.red);
    } else {
        painter.rect_filled(rect, 4.0, p.red_dim);
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if is_clipping { "CLIP!" } else { "CLIP" },
        egui::FontId::monospace(10.5),
        if is_clipping {
            Color32::WHITE
        } else {
            p.text_muted
        },
    );
}

/// Skeuomorphes Vintage-Analog-VU-Meter (Creme-Zifferblatt im Silverface Light Mode, warmes Glühen im Obsidian Dark Mode).
pub fn render_vintage_vu_meter(
    ui: &mut Ui,
    meter: &MeterLevels,
    label: &str,
    reset_label: &str,
    clipping_label: &str,
) -> bool {
    let desired = Vec2::new(ui.available_width().max(260.0), 86.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());

    response.widget_info(|| {
        let clipping = if meter.is_clipping {
            format!(". {clipping_label}")
        } else {
            String::new()
        };
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!(
                "{label}. VU L {}. VU R {}{clipping}. {reset_label}",
                format_db(meter.peak_l_db),
                format_db(meter.peak_r_db)
            ),
        )
    });

    let painter = ui.painter();
    let gap = 8.0;
    let dial_w = ((rect.width() - gap) * 0.5).max(120.0);
    let left_rect = Rect::from_min_size(rect.min, Vec2::new(dial_w, rect.height()));
    let right_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + dial_w + gap, rect.min.y),
        Vec2::new(dial_w, rect.height()),
    );

    let smooth = |id: egui::Id, target_db: f32| -> f32 {
        ui.data_mut(|data| {
            let prev = data.get_temp::<f32>(id).unwrap_or(-60.0);
            let alpha = if target_db > prev { 0.42 } else { 0.16 };
            let next = prev + (target_db - prev) * alpha;
            data.insert_temp(id, next);
            next
        })
    };
    let vu_l = smooth(
        response.id.with("vu_l"),
        meter.rms_l_db.max(meter.peak_l_db - 4.0),
    );
    let vu_r = smooth(
        response.id.with("vu_r"),
        meter.rms_r_db.max(meter.peak_r_db - 4.0),
    );

    draw_single_vu_dial(
        painter,
        left_rect,
        "VU L",
        vu_l,
        meter.peak_l_db,
        meter.is_clipping,
    );
    draw_single_vu_dial(
        painter,
        right_rect,
        "VU R",
        vu_r,
        meter.peak_r_db,
        meter.is_clipping,
    );

    draw_keyboard_focus(ui, &response);
    response.on_hover_text(reset_label).clicked()
}

fn draw_single_vu_dial(
    painter: &egui::Painter,
    rect: Rect,
    title: &str,
    vu_db: f32,
    peak_db: f32,
    clipping: bool,
) {
    let p = palette();
    paint_rounded_vertical_gradient(painter, rect, RADIUS_WELL, p.vu_face_top, p.vu_face_bottom);
    painter.rect_stroke(
        rect.shrink(0.5),
        RADIUS_WELL,
        Stroke::new(0.75, p.border_mid),
    );

    // Titel & Peak-LED oben
    painter.text(
        Pos2::new(rect.min.x + 9.0, rect.min.y + 7.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::monospace(10.0),
        p.vu_scale,
    );

    let peak_hot = clipping || peak_db >= -1.0;
    let led_center = Pos2::new(rect.max.x - 12.0, rect.min.y + 12.0);
    painter.circle_filled(
        led_center,
        3.5,
        if peak_hot { p.red } else { p.red_dim },
    );
    painter.text(
        Pos2::new(led_center.x - 7.0, led_center.y),
        egui::Align2::RIGHT_CENTER,
        "PEAK",
        egui::FontId::monospace(8.5),
        if peak_hot { p.red } else { p.vu_scale },
    );

    // Skalenbogen (-42° bis +42°)
    let pivot = Pos2::new(rect.center().x, rect.max.y + 20.0);
    let arc_r = (rect.height() - 10.0).min(rect.width() * 0.44) + 12.0;
    let angle_for = |frac: f32| -> f32 {
        let f = frac.clamp(0.0, 1.0);
        (-42.0 + f * 84.0).to_radians()
    };
    let pt = |frac: f32, r: f32| -> Pos2 {
        let a = angle_for(frac);
        Pos2::new(pivot.x + a.sin() * r, pivot.y - a.cos() * r)
    };

    let steps = 28;
    for i in 0..steps {
        let f0 = i as f32 / steps as f32;
        let f1 = (i + 1) as f32 / steps as f32;
        let col = if f0 >= 0.72 { p.red } else { p.vu_scale };
        painter.line_segment([pt(f0, arc_r), pt(f1, arc_r)], Stroke::new(1.4, col));
    }

    for &(frac, lbl, is_red) in &[
        (0.05, "-20", false),
        (0.26, "-10", false),
        (0.46, "-5", false),
        (0.60, "-3", false),
        (0.72, "0", true),
        (0.94, "+3", true),
    ] {
        let col = if is_red { p.red } else { p.vu_scale };
        painter.line_segment(
            [pt(frac, arc_r - 5.0), pt(frac, arc_r + 2.0)],
            Stroke::new(1.2, col),
        );
        painter.text(
            pt(frac, arc_r + 9.0),
            egui::Align2::CENTER_CENTER,
            lbl,
            egui::FontId::monospace(8.5),
            col,
        );
    }

    let vu_frac = ((vu_db + 38.0) / 38.0).clamp(0.0, 1.0);
    let needle_tip = pt(vu_frac, arc_r + 1.0);
    let needle_base = pt(vu_frac, (arc_r - 36.0).max(12.0));

    painter.line_segment(
        [
            needle_base + Vec2::new(1.2, 1.2),
            needle_tip + Vec2::new(1.2, 1.2),
        ],
        Stroke::new(
            1.8,
            Color32::from_rgba_unmultiplied(0, 0, 0, if p.is_dark { 130 } else { 42 }),
        ),
    );
    painter.line_segment(
        [needle_base, needle_tip],
        Stroke::new(1.8, if vu_frac >= 0.72 { p.red } else { p.vu_needle }),
    );

    let val_txt = if peak_db <= -59.5 {
        "-∞ dB".to_string()
    } else {
        format!("{peak_db:+.1} dB")
    };
    painter.text(
        Pos2::new(rect.max.x - 8.0, rect.max.y - 6.0),
        egui::Align2::RIGHT_BOTTOM,
        val_txt,
        egui::FontId::monospace(9.5),
        if peak_hot { p.red } else { p.amber },
    );
}

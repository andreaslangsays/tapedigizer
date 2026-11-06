use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use crate::audio::recorder::StereoWaveformPoint;
use crate::ui::theme::{
    draw_recessed_well, BG_CARD, BG_INSET, BORDER_DARK, BORDER_LIGHT, LED_AMBER,
    LED_GREEN, TEXT_MUTED, TEXT_SECONDARY, WAVE_LEFT, WAVE_RIGHT,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditTool {
    Pointer, // Standard: Klick in Wellenform/Ruler setzt Playhead, Marker-Fähnchen verschieben Marker
    Scissor, // ✂️ Scheren-Werkzeug: Klick setzt Trenner (Schnitt-Marker)
    Select,  // ⬚ Bereichsauswahl: Ziehen wählt einen Zeitbereich für Lupe & Transienten aus
}

pub struct WaveformInteraction {
    pub seek_to_seconds: Option<f64>,
    pub add_marker_at_seconds: Option<f64>,
    pub dragged_marker: Option<(usize, f64)>,
    pub delete_marker_idx: Option<usize>,
    pub hovered_time_sec: Option<f64>,
    pub mouse_pos_in_waveform: Option<Pos2>,
    pub is_mouse_over_waveform: bool,
    pub clicked_segment_idx: Option<usize>,
    pub new_selected_region: Option<(f64, f64)>,
}

pub fn render_stereo_waveform(
    ui: &mut Ui,
    waveform_data: &[StereoWaveformPoint],
    total_duration_sec: f64,
    current_playhead_sec: f64,
    markers: &[f64],
    pause_indices: &[usize],
    selected_marker_idx: &mut Option<usize>,
    selected_region: Option<(f64, f64)>,
    zoom_factor: &mut f32,
    vertical_zoom: &mut f32,
    waveform_height: f32,
    scroll_offset_frac: &mut f32,
    active_tool: EditTool,
    hover_audition_active: bool,
    hover_audition_time: Option<f64>,
) -> WaveformInteraction {
    let mut interaction = WaveformInteraction {
        seek_to_seconds: None,
        add_marker_at_seconds: None,
        dragged_marker: None,
        delete_marker_idx: None,
        hovered_time_sec: None,
        mouse_pos_in_waveform: None,
        is_mouse_over_waveform: false,
        clicked_segment_idx: None,
        new_selected_region: None,
    };

    let desired_height = waveform_height.clamp(200.0, 520.0);
    let available_width = ui.available_width().max(380.0);
    let size = Vec2::new(available_width, desired_height);

    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter();

    // Studio Rack Plate Housing
    painter.rect_filled(rect, 8.0, BG_CARD);
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, BORDER_DARK));

    let header_width = 54.0;
    let wf_left = rect.min.x + header_width + 8.0;
    let wf_right = rect.max.x - 12.0;
    let wf_width = (wf_right - wf_left).max(10.0);

    let ruler_h = 24.0;
    let overview_h = 22.0;
    let padding_y = 26.0;
    let wf_top = rect.min.y + 10.0;
    let wf_h = (desired_height - ruler_h - overview_h - padding_y).max(110.0);
    let wf_bottom = wf_top + wf_h;
    let wf_rect = Rect::from_min_max(Pos2::new(wf_left, wf_top), Pos2::new(wf_right, wf_bottom));

    let ruler_top = wf_bottom + 4.0;
    let ruler_bottom = ruler_top + ruler_h;
    let ruler_rect = Rect::from_min_max(Pos2::new(wf_left, ruler_top), Pos2::new(wf_right, ruler_bottom));

    let overview_top = ruler_bottom + 4.0;
    let overview_bottom = overview_top + overview_h;
    let overview_rect = Rect::from_min_max(Pos2::new(wf_left, overview_top), Pos2::new(wf_right, overview_bottom));

    // Two Stereo Lanes
    let lane_gap = 6.0;
    let lane_h = (wf_rect.height() - lane_gap) * 0.5;

    let lane_l_top = wf_rect.min.y;
    let lane_l_bot = lane_l_top + lane_h;
    let lane_l_center = lane_l_top + lane_h * 0.5;
    let lane_l_rect = Rect::from_min_max(Pos2::new(wf_rect.min.x, lane_l_top), Pos2::new(wf_rect.max.x, lane_l_bot));

    let lane_r_top = lane_l_bot + lane_gap;
    let lane_r_bot = lane_r_top + lane_h;
    let lane_r_center = lane_r_top + lane_h * 0.5;
    let lane_r_rect = Rect::from_min_max(Pos2::new(wf_rect.min.x, lane_r_top), Pos2::new(wf_rect.max.x, lane_r_bot));

    // Recessed track wells
    draw_recessed_well(painter, lane_l_rect, 4.0, BG_INSET);
    draw_recessed_well(painter, lane_r_rect, 4.0, BG_INSET);

    // Track Header Badges on Left
    let header_l = Rect::from_min_max(Pos2::new(rect.min.x + 8.0, lane_l_top), Pos2::new(rect.min.x + header_width, lane_l_bot));
    draw_track_header(painter, header_l, "L", "Track 1", WAVE_LEFT);

    let header_r = Rect::from_min_max(Pos2::new(rect.min.x + 8.0, lane_r_top), Pos2::new(rect.min.x + header_width, lane_r_bot));
    draw_track_header(painter, header_r, "R", "Track 2", WAVE_RIGHT);

    // Center baselines
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_l_center), Pos2::new(wf_rect.max.x, lane_l_center)],
        Stroke::new(1.0, Color32::from_rgb(38, 42, 52)),
    );
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_r_center), Pos2::new(wf_rect.max.x, lane_r_center)],
        Stroke::new(1.0, Color32::from_rgb(38, 42, 52)),
    );

    // -------------------------------------------------------------
    // Zoom & Horizontal Scrolling via Mouse Wheel & Gestures
    // -------------------------------------------------------------
    let (scroll_delta, alt_pressed, shift_pressed, cmd_pressed) = ui.input(|i| {
        let delta = if i.smooth_scroll_delta != Vec2::ZERO {
            i.smooth_scroll_delta
        } else {
            i.raw_scroll_delta
        };
        (delta, i.modifiers.alt, i.modifiers.shift, i.modifiers.command)
    });

    let is_hovered = response.hovered();
    let mouse_pos_opt = ui.input(|i| i.pointer.hover_pos()).or_else(|| response.interact_pointer_pos());

    // 1. Mouse Wheel Zoom (Option / Alt + Wheel)
    if is_hovered && alt_pressed && (scroll_delta.y.abs() > 0.1 || scroll_delta.x.abs() > 0.1) {
        if shift_pressed {
            // Shift + Alt + Wheel: Vertical amplitude zoom!
            let delta = if scroll_delta.y.abs() > 0.1 { scroll_delta.y } else { scroll_delta.x };
            let v_mult = if delta > 0.0 { 1.2 } else { 1.0 / 1.2 };
            *vertical_zoom = (*vertical_zoom * v_mult).clamp(1.0, 16.0);
        } else {
            let old_zoom = *zoom_factor;
            let delta = if scroll_delta.y.abs() > 0.1 { scroll_delta.y } else { scroll_delta.x };
            let zoom_mult = if delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
            let new_zoom = (old_zoom * zoom_mult).clamp(1.0, 30.0);

            if (new_zoom - old_zoom).abs() > 0.001 {
                if let Some(mouse_pos) = mouse_pos_opt {
                    let rel_x = ((mouse_pos.x - wf_rect.min.x) / wf_width).clamp(0.0, 1.0) as f64;
                    let old_span = 1.0 / old_zoom as f64;
                    let new_span = 1.0 / new_zoom as f64;
                    let t_frac = (*scroll_offset_frac as f64) + rel_x * old_span;
                    let new_scroll = t_frac - rel_x * new_span;
                    let max_scroll = (1.0 - new_span).max(0.0);
                    *scroll_offset_frac = new_scroll.clamp(0.0, max_scroll) as f32;
                }
                *zoom_factor = new_zoom;
            }
        }
    }
    // 2. Mouse Wheel Horizontal Scroll (Normal Scroll without Option)
    else if is_hovered && !alt_pressed && (scroll_delta.x.abs() > 0.1 || scroll_delta.y.abs() > 0.1) {
        let delta = if scroll_delta.x.abs() > 0.1 { scroll_delta.x } else { scroll_delta.y };
        let view_width_frac = 1.0 / *zoom_factor;
        let max_scroll = (1.0 - view_width_frac).max(0.0);
        let scroll_step = (delta / wf_width) * 0.8;
        *scroll_offset_frac = (*scroll_offset_frac - scroll_step).clamp(0.0, max_scroll);
    }

    // 3. Middle-Mouse Drag or Secondary Drag to Pan
    if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Secondary) {
        let drag_delta = response.drag_delta();
        let view_width_frac = 1.0 / *zoom_factor;
        let max_scroll = (1.0 - view_width_frac).max(0.0);
        let scroll_step = drag_delta.x / wf_width * view_width_frac;
        *scroll_offset_frac = (*scroll_offset_frac - scroll_step).clamp(0.0, max_scroll);
    }

    // Viewport calculations
    let zoom = zoom_factor.max(1.0);
    let view_width_frac = 1.0 / zoom;
    let max_scroll = (1.0 - view_width_frac).max(0.0);
    *scroll_offset_frac = scroll_offset_frac.clamp(0.0, max_scroll);

    let view_start_frac = *scroll_offset_frac as f64;
    let view_end_frac = (view_start_frac + view_width_frac as f64).min(1.0);

    let time_to_screen_x = |t_sec: f64| -> f32 {
        if total_duration_sec <= 0.0 {
            return wf_rect.min.x;
        }
        let frac = t_sec / total_duration_sec;
        let view_span = (view_end_frac - view_start_frac).max(0.0001);
        let rel_x = (frac - view_start_frac) / view_span;
        wf_rect.min.x + (rel_x as f32) * wf_width
    };

    let screen_x_to_time = |x: f32| -> f64 {
        if wf_width <= 0.0 || total_duration_sec <= 0.0 {
            return 0.0;
        }
        let rel_x = ((x - wf_rect.min.x) / wf_width).clamp(0.0, 1.0) as f64;
        let view_span = (view_end_frac - view_start_frac).max(0.0001);
        let frac = (view_start_frac + rel_x * view_span).clamp(0.0, 1.0);
        frac * total_duration_sec
    };

    // -------------------------------------------------------------
    // Track Segment Backgrounds (Track Blocks & Excluded Pauses)
    // -------------------------------------------------------------
    if total_duration_sec > 0.0 {
        let mut block_points = vec![0.0];
        for &m in markers {
            if m > 0.0 && m < total_duration_sec {
                block_points.push(m);
            }
        }
        block_points.push(total_duration_sec);

        let mut seg_active_count = 0;
        for i in 0..block_points.len() - 1 {
            let x1 = time_to_screen_x(block_points[i]).clamp(wf_rect.min.x, wf_rect.max.x);
            let x2 = time_to_screen_x(block_points[i + 1]).clamp(wf_rect.min.x, wf_rect.max.x);
            let is_p = pause_indices.contains(&i);

            if x2 > x1 + 1.0 {
                let seg_r = Rect::from_min_max(Pos2::new(x1, wf_rect.min.y), Pos2::new(x2, wf_rect.max.y));
                if is_p {
                    painter.rect_filled(seg_r, 0.0, Color32::from_rgba_unmultiplied(160, 40, 20, 32));
                    // Diagonal hatched pattern
                    let mut hx = x1;
                    while hx < x2 + seg_r.height() {
                        let p1 = Pos2::new(hx, seg_r.max.y);
                        let p2 = Pos2::new(hx - seg_r.height(), seg_r.min.y);
                        let cp1 = Pos2::new(p1.x.clamp(x1, x2), p1.y);
                        let cp2 = Pos2::new(p2.x.clamp(x1, x2), p2.y);
                        if (cp1.x - cp2.x).abs() > 1.0 {
                            painter.line_segment([cp1, cp2], Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 60, 30, 25)));
                        }
                        hx += 16.0;
                    }
                    if x2 - x1 > 90.0 {
                        let badge = Rect::from_center_size(
                            Pos2::new((x1 + x2) * 0.5, wf_rect.min.y + 14.0),
                            Vec2::new(140.0, 18.0),
                        );
                        painter.rect_filled(badge, 3.0, Color32::from_rgba_unmultiplied(35, 18, 14, 210));
                        painter.rect_stroke(badge, 3.0, Stroke::new(1.0, LED_AMBER));
                        painter.text(badge.center(), egui::Align2::CENTER_CENTER, "⏸ PAUSE (WEGGELASSEN)", egui::FontId::monospace(9.0), LED_AMBER);
                    }
                } else {
                    seg_active_count += 1;
                    let tint = if seg_active_count % 2 == 1 {
                        Color32::from_rgba_unmultiplied(37, 99, 235, 14)
                    } else {
                        Color32::from_rgba_unmultiplied(16, 149, 193, 14)
                    };
                    painter.rect_filled(seg_r, 0.0, tint);
                    if x2 - x1 > 50.0 {
                        painter.text(
                            Pos2::new(x1 + 6.0, wf_rect.min.y + 12.0),
                            egui::Align2::LEFT_CENTER,
                            format!("TRACK {:02}", seg_active_count),
                            egui::FontId::monospace(9.5),
                            Color32::from_rgba_unmultiplied(190, 210, 240, 130),
                        );
                    }
                }
            }
        }
    }

    // Highlight Selected Region (Lupe Scope Target Window)
    if let Some((r_start, r_end)) = selected_region {
        if total_duration_sec > 0.0 && r_end > r_start {
            let rx1 = time_to_screen_x(r_start).clamp(wf_rect.min.x, wf_rect.max.x);
            let rx2 = time_to_screen_x(r_end).clamp(wf_rect.min.x, wf_rect.max.x);
            if rx2 > rx1 + 1.0 {
                let reg_r = Rect::from_min_max(Pos2::new(rx1, wf_rect.min.y), Pos2::new(rx2, ruler_rect.max.y));
                painter.rect_filled(reg_r, 0.0, Color32::from_rgba_unmultiplied(255, 215, 0, 32));
                painter.line_segment([Pos2::new(rx1, wf_rect.min.y), Pos2::new(rx1, ruler_rect.max.y)], Stroke::new(2.0, Color32::from_rgb(255, 215, 0)));
                painter.line_segment([Pos2::new(rx2, wf_rect.min.y), Pos2::new(rx2, ruler_rect.max.y)], Stroke::new(2.0, Color32::from_rgb(255, 215, 0)));

                // Region header badge
                let span_sec = r_end - r_start;
                let badge_w = 94.0_f32;
                let badge_x = ((rx1 + rx2) * 0.5).clamp(wf_rect.min.x + badge_w * 0.5, wf_rect.max.x - badge_w * 0.5);
                let badge_r = Rect::from_center_size(Pos2::new(badge_x, wf_rect.min.y + 12.0), Vec2::new(badge_w, 16.0));
                painter.rect_filled(badge_r, 3.0, Color32::from_rgba_unmultiplied(30, 25, 8, 225));
                painter.rect_stroke(badge_r, 3.0, Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                painter.text(badge_r.center(), egui::Align2::CENTER_CENTER, format!("🔍 LUPE: {:.2}s", span_sec), egui::FontId::monospace(8.5), Color32::from_rgb(255, 215, 0));
            }
        }
    }

    // -------------------------------------------------------------
    // Render Dual Waveforms (with Pixel-Binning & Zero Baselines)
    // -------------------------------------------------------------
    // Center zero baselines for L & R
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_l_center), Pos2::new(wf_rect.max.x, lane_l_center)],
        Stroke::new(1.0, Color32::from_rgb(32, 38, 50)),
    );
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_r_center), Pos2::new(wf_rect.max.x, lane_r_center)],
        Stroke::new(1.0, Color32::from_rgb(32, 38, 50)),
    );

    let n_points = waveform_data.len();
    if n_points > 0 {
        let start_pt = ((view_start_frac * n_points as f64).floor() as usize).min(n_points);
        let end_pt = ((view_end_frac * n_points as f64).ceil() as usize).min(n_points).max(start_pt + 1);
        let visible_count = end_pt - start_pt;

        let num_bars = ((wf_width / 2.0).floor() as usize).clamp(20, 1600);
        let bar_step = wf_width / num_bars as f32;
        let bar_w = (bar_step * 0.75).clamp(1.0, 3.5);
        let amp_scale = (*vertical_zoom).clamp(1.0, 16.0);

        for bar_idx in 0..num_bars {
            let p0 = start_pt + ((bar_idx as f64 * visible_count as f64) / num_bars as f64) as usize;
            let p1 = (start_pt + (((bar_idx + 1) as f64 * visible_count as f64) / num_bars as f64).ceil() as usize)
                .min(end_pt)
                .max(p0 + 1);

            let mut min_l = 0.0_f32;
            let mut max_l = 0.0_f32;
            let mut min_r = 0.0_f32;
            let mut max_r = 0.0_f32;

            for p in &waveform_data[p0..p1] {
                if p.min_l < min_l { min_l = p.min_l; }
                if p.max_l > max_l { max_l = p.max_l; }
                if p.min_r < min_r { min_r = p.min_r; }
                if p.max_r > max_r { max_r = p.max_r; }
            }

            let x = wf_rect.min.x + bar_idx as f32 * bar_step + bar_step * 0.5;

            // Lane L
            let top_l = lane_l_center - ((max_l * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_l = lane_l_center - ((min_l * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.46));
            let h_l = (bot_l - top_l).max(1.0);
            painter.line_segment(
                [Pos2::new(x, top_l), Pos2::new(x, top_l + h_l)],
                Stroke::new(bar_w, WAVE_LEFT),
            );

            // Lane R
            let top_r = lane_r_center - ((max_r * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_r = lane_r_center - ((min_r * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.46));
            let h_r = (bot_r - top_r).max(1.0);
            painter.line_segment(
                [Pos2::new(x, top_r), Pos2::new(x, top_r + h_r)],
                Stroke::new(bar_w, WAVE_RIGHT),
            );
        }
    } else {
        painter.text(
            wf_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Keine Audiodaten — Aufnahme in Tab 1 starten",
            egui::FontId::proportional(14.0),
            TEXT_MUTED,
        );
    }

    // -------------------------------------------------------------
    // Timeline Ruler Bar (Dedicated Playhead scrub zone)
    // -------------------------------------------------------------
    draw_recessed_well(painter, ruler_rect, 3.0, Color32::from_rgb(26, 27, 32));
    let ruler_y = ruler_rect.min.y + 11.0;

    if total_duration_sec > 1.0 {
        let visible_duration = (view_end_frac - view_start_frac) * total_duration_sec;
        let step_sec = if visible_duration < 10.0 {
            1.0
        } else if visible_duration < 30.0 {
            2.0
        } else if visible_duration < 90.0 {
            5.0
        } else if visible_duration < 300.0 {
            15.0
        } else if visible_duration < 1200.0 {
            60.0
        } else {
            300.0
        };

        let start_time = (view_start_frac * total_duration_sec / step_sec).floor() * step_sec;
        let end_time = (view_end_frac * total_duration_sec).min(total_duration_sec);

        let mut t = start_time;
        while t <= end_time {
            let tx = time_to_screen_x(t);
            if tx >= wf_rect.min.x - 2.0 && tx <= wf_rect.max.x + 2.0 {
                // Ticks
                painter.line_segment(
                    [Pos2::new(tx, ruler_rect.min.y + 2.0), Pos2::new(tx, ruler_rect.min.y + 7.0)],
                    Stroke::new(1.0, BORDER_LIGHT),
                );

                let mins = (t / 60.0).floor() as u64;
                let secs = (t % 60.0).floor() as u64;
                let label = format!("{:02}:{:02}", mins, secs);

                painter.text(
                    Pos2::new(tx, ruler_rect.min.y + 8.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    egui::FontId::monospace(9.5),
                    TEXT_SECONDARY,
                );
            }
            t += step_sec;
        }
    }

    // Label on left of ruler indicating role
    painter.text(
        Pos2::new(rect.min.x + 10.0, ruler_y),
        egui::Align2::LEFT_CENTER,
        "ZEIT",
        egui::FontId::monospace(9.0),
        TEXT_MUTED,
    );

    // -------------------------------------------------------------
    // Minimap / Overview Scrollbar (Shows whole tape & drag window)
    // -------------------------------------------------------------
    draw_recessed_well(painter, overview_rect, 3.0, Color32::from_rgb(20, 20, 23));
    painter.text(
        Pos2::new(rect.min.x + 10.0, overview_rect.center().y),
        egui::Align2::LEFT_CENTER,
        "ÜBERSICHT",
        egui::FontId::monospace(8.0),
        TEXT_MUTED,
    );

    // Overview track blocks representation (Gesamtansicht der geschnittenen Stücke)
    let ov_w = overview_rect.width();
    if total_duration_sec > 0.0 {
        let mut ov_points = vec![0.0];
        for &m in markers {
            if m > 0.0 && m < total_duration_sec {
                ov_points.push(m);
            }
        }
        ov_points.push(total_duration_sec);

        let mut active_idx = 0;
        for i in 0..ov_points.len() - 1 {
            let x1 = overview_rect.min.x + (ov_points[i] / total_duration_sec) as f32 * ov_w;
            let x2 = overview_rect.min.x + (ov_points[i + 1] / total_duration_sec) as f32 * ov_w;
            let is_p = pause_indices.contains(&i);

            if x2 > x1 + 1.0 {
                let block_r = Rect::from_min_max(Pos2::new(x1, overview_rect.min.y + 1.0), Pos2::new(x2, overview_rect.max.y - 1.0));
                let (fill, stroke_col, label) = if is_p {
                    (
                        Color32::from_rgba_unmultiplied(160, 40, 20, 90),
                        Color32::from_rgb(200, 70, 40),
                        "⏸".to_string(),
                    )
                } else {
                    active_idx += 1;
                    let bg = if active_idx % 2 == 1 {
                        Color32::from_rgba_unmultiplied(37, 99, 235, 75)
                    } else {
                        Color32::from_rgba_unmultiplied(16, 149, 193, 75)
                    };
                    (bg, Color32::from_rgb(96, 165, 250), format!("#{}", active_idx))
                };

                painter.rect_filled(block_r, 2.0, fill);
                painter.rect_stroke(block_r, 2.0, Stroke::new(1.0, stroke_col.gamma_multiply(0.4)));

                if block_r.width() > 16.0 {
                    painter.text(
                        block_r.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::monospace(8.5),
                        Color32::WHITE,
                    );
                }
            }
        }

        // Region indicator on overview bar
        if let Some((r_start, r_end)) = selected_region {
            if r_end > r_start {
                let orx1 = overview_rect.min.x + (r_start / total_duration_sec) as f32 * ov_w;
                let orx2 = overview_rect.min.x + (r_end / total_duration_sec) as f32 * ov_w;
                if orx2 > orx1 + 1.0 {
                    let ob_r = Rect::from_min_max(Pos2::new(orx1, overview_rect.min.y), Pos2::new(orx2, overview_rect.max.y));
                    painter.rect_filled(ob_r, 2.0, Color32::from_rgba_unmultiplied(255, 215, 0, 90));
                    painter.rect_stroke(ob_r, 2.0, Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                }
            }
        }
    }

    // Draggable viewport thumb window
    let thumb_left = overview_rect.min.x + (view_start_frac as f32) * ov_w;
    let thumb_right = overview_rect.min.x + (view_end_frac as f32) * ov_w;
    let thumb_rect = Rect::from_min_max(
        Pos2::new(thumb_left, overview_rect.min.y + 1.0),
        Pos2::new(thumb_right.max(thumb_left + 12.0), overview_rect.max.y - 1.0),
    );

    let is_thumb_hovered = mouse_pos_opt.map(|p| thumb_rect.contains(p)).unwrap_or(false);
    let thumb_fill = if is_thumb_hovered {
        Color32::from_rgba_unmultiplied(56, 189, 248, 60)
    } else {
        Color32::from_rgba_unmultiplied(56, 189, 248, 35)
    };
    painter.rect_filled(thumb_rect, 3.0, thumb_fill);
    painter.rect_stroke(thumb_rect, 3.0, Stroke::new(1.0, Color32::from_rgb(56, 189, 248)));

    // Handle Overview bar clicking and dragging
    if let Some(pos) = mouse_pos_opt {
        if overview_rect.contains(pos) && (response.clicked() || response.dragged()) {
            let clicked_frac = ((pos.x - overview_rect.min.x) / ov_w).clamp(0.0, 1.0) as f64;
            let new_scroll = (clicked_frac - (view_width_frac as f64) * 0.5).clamp(0.0, max_scroll as f64);
            *scroll_offset_frac = new_scroll as f32;

            if response.clicked() && total_duration_sec > 0.0 {
                let clicked_t = clicked_frac * total_duration_sec;
                let mut ov_points = vec![0.0];
                for &m in markers {
                    if m > 0.0 && m < total_duration_sec {
                        ov_points.push(m);
                    }
                }
                ov_points.push(total_duration_sec);
                for i in 0..ov_points.len() - 1 {
                    if clicked_t >= ov_points[i] && clicked_t < ov_points[i + 1] {
                        interaction.clicked_segment_idx = Some(i);
                        break;
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------
    // Tape Splicing Markers (Golden vertical line cutting through tracks)
    // -------------------------------------------------------------
    let mut hovered_marker_idx = None;

    for (idx, &m_sec) in markers.iter().enumerate() {
        if total_duration_sec <= 0.0 { break; }
        let mx = time_to_screen_x(m_sec);

        if mx >= wf_rect.min.x - 2.0 && mx <= wf_rect.max.x + 2.0 {
            let is_selected = *selected_marker_idx == Some(idx);
            let marker_color = if is_selected {
                Color32::from_rgb(255, 215, 0) // Bright Gold
            } else {
                LED_AMBER
            };

            // Vertical razor cut line
            painter.line_segment(
                [Pos2::new(mx, wf_rect.min.y), Pos2::new(mx, ruler_rect.max.y)],
                Stroke::new(2.0, marker_color),
            );

            // Flag at top
            let flag_w = 42.0;
            let flag_h = 20.0;
            let flag_rect = Rect::from_center_size(
                Pos2::new(mx, wf_rect.min.y + 11.0),
                Vec2::new(flag_w, flag_h),
            );

            if let Some(pos) = mouse_pos_opt {
                if flag_rect.expand(4.0).contains(pos) || (pos.x - mx).abs() < 8.0 && pos.y >= wf_rect.min.y && pos.y <= ruler_rect.max.y {
                    hovered_marker_idx = Some(idx);
                }
            }

            let flag_bg = if is_selected {
                Color32::from_rgb(255, 230, 80)
            } else {
                marker_color
            };

            painter.rect_filled(flag_rect, 4.0, flag_bg);
            painter.rect_stroke(flag_rect, 4.0, Stroke::new(1.0, BORDER_DARK));

            // Scissor symbol and Track number
            painter.text(
                flag_rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("T{:02}", idx + 2),
                egui::FontId::monospace(10.5),
                Color32::BLACK,
            );

            // Bottom anchor tab on ruler
            let anchor_rect = Rect::from_center_size(
                Pos2::new(mx, ruler_rect.max.y - 4.0),
                Vec2::new(10.0, 8.0),
            );
            painter.rect_filled(anchor_rect, 2.0, marker_color);
        }
    }

    // -------------------------------------------------------------
    // Playhead (Continuous white laser cursor)
    // -------------------------------------------------------------
    if total_duration_sec > 0.0 {
        let px = time_to_screen_x(current_playhead_sec);
        if px >= wf_rect.min.x - 2.0 && px <= wf_rect.max.x + 2.0 {
            // White playhead line
            painter.line_segment(
                [Pos2::new(px, wf_rect.min.y), Pos2::new(px, ruler_rect.max.y)],
                Stroke::new(2.0, Color32::WHITE),
            );

            // Inverted triangle playhead handle on ruler
            let triangle_pts = vec![
                Pos2::new(px - 7.0, ruler_rect.min.y),
                Pos2::new(px + 7.0, ruler_rect.min.y),
                Pos2::new(px, ruler_rect.min.y + 10.0),
            ];
            painter.add(egui::Shape::convex_polygon(triangle_pts, Color32::WHITE, Stroke::NONE));

            // Top flag
            let top_pts = vec![
                Pos2::new(px - 5.0, wf_rect.min.y),
                Pos2::new(px + 5.0, wf_rect.min.y),
                Pos2::new(px, wf_rect.min.y + 7.0),
            ];
            painter.add(egui::Shape::convex_polygon(top_pts, Color32::WHITE, Stroke::NONE));
        }
    }

    // -------------------------------------------------------------
    // Hover Audition Beacon / Glowing Vorhör-Indikator
    // -------------------------------------------------------------
    if hover_audition_active {
        if let Some(audition_t) = hover_audition_time {
            let ax = time_to_screen_x(audition_t);
            if ax >= wf_rect.min.x && ax <= wf_rect.max.x {
                // Pulsing green/cyan audition playhead
                painter.line_segment(
                    [Pos2::new(ax, wf_rect.min.y), Pos2::new(ax, ruler_rect.max.y)],
                    Stroke::new(2.0, LED_GREEN),
                );

                // Audition badge
                let mins = (audition_t / 60.0).floor() as u64;
                let secs = (audition_t % 60.0).floor() as u64;
                let badge_rect = Rect::from_center_size(
                    Pos2::new(ax, wf_rect.min.y + 36.0),
                    Vec2::new(100.0, 20.0),
                );
                painter.rect_filled(badge_rect, 4.0, Color32::from_rgb(16, 50, 30));
                painter.rect_stroke(badge_rect, 4.0, Stroke::new(1.0, LED_GREEN));
                painter.text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("VORHÖREN {:02}:{:02}", mins, secs),
                    egui::FontId::monospace(9.5),
                    LED_GREEN,
                );
            }
        }
    }

    // -------------------------------------------------------------
    // Mouse Pointer Inspection, Tool Cursors & Cutting Guide
    // -------------------------------------------------------------
    let is_scissor = active_tool == EditTool::Scissor || shift_pressed || cmd_pressed;

    if let Some(pos) = mouse_pos_opt {
        if wf_rect.contains(pos) {
            interaction.is_mouse_over_waveform = true;
            interaction.mouse_pos_in_waveform = Some(pos);
            let mouse_t = screen_x_to_time(pos.x);
            interaction.hovered_time_sec = Some(mouse_t);

            // Dynamic Cutting Guide Line in Scissor Mode
            if is_scissor {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);

                // Dashed vertical cutting line
                let dash_len = 5.0;
                let mut cy = wf_rect.min.y;
                while cy < wf_rect.max.y {
                    painter.line_segment(
                        [Pos2::new(pos.x, cy), Pos2::new(pos.x, (cy + dash_len).min(wf_rect.max.y))],
                        Stroke::new(1.5, LED_AMBER),
                    );
                    cy += dash_len * 2.0;
                }

                // Scissor Flag at mouse
                let guide_tag = Rect::from_center_size(
                    Pos2::new(pos.x, pos.y - 18.0),
                    Vec2::new(110.0, 20.0),
                );
                let mins = (mouse_t / 60.0).floor() as u64;
                let secs = (mouse_t % 60.0).floor() as u64;
                painter.rect_filled(guide_tag, 4.0, Color32::from_rgb(45, 35, 10));
                painter.rect_stroke(guide_tag, 4.0, Stroke::new(1.0, LED_AMBER));
                painter.text(
                    guide_tag.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("TRENNER {:02}:{:02}", mins, secs),
                    egui::FontId::monospace(9.5),
                    LED_AMBER,
                );
            } else if active_tool == EditTool::Select || ui.input(|i| i.modifiers.shift) {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            } else if hovered_marker_idx.is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
        } else if ruler_rect.contains(pos) {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
    }

    // -------------------------------------------------------------
    // Distinct Click & Drag Actions:
    // 1. Ruler: strictly sets Playhead
    // 2. Waveform in Scissor Mode: strictly adds Split Marker
    // 3. Waveform in Select Mode (or Shift+Drag): selects Region for Lupe & Transients
    // 4. Waveform in Pointer Mode: drag marker handle or seek playhead
    // -------------------------------------------------------------
    if response.clicked() || response.dragged() {
        if let Some(pos) = mouse_pos_opt {
            let target_time = screen_x_to_time(pos.x);

            // A) Click/Drag in Timeline Ruler -> Sets Playhead
            if ruler_rect.contains(pos) {
                interaction.seek_to_seconds = Some(target_time);
            }
            // B) Click in Waveform
            else if wf_rect.contains(pos) {
                // In Scissor mode: click places a marker
                if is_scissor {
                    if response.clicked() {
                        interaction.add_marker_at_seconds = Some(target_time);
                    }
                }
                // In Select mode or with Shift held: drag selects a region for Lupe & Transients
                else if active_tool == EditTool::Select || ui.input(|i| i.modifiers.shift) {
                    if response.dragged() {
                        if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                            let t_start = screen_x_to_time(origin.x);
                            let t_curr = target_time;
                            if (t_curr - t_start).abs() > 0.05 {
                                interaction.new_selected_region = Some((t_start.min(t_curr), t_start.max(t_curr)));
                            }
                        }
                    } else if response.clicked() {
                        // Click in select mode selects the clicked track segment
                        let mut blk = vec![0.0];
                        for &m in markers {
                            if m > 0.0 && m < total_duration_sec {
                                blk.push(m);
                            }
                        }
                        blk.push(total_duration_sec);
                        for i in 0..blk.len() - 1 {
                            if target_time >= blk[i] && target_time < blk[i + 1] {
                                interaction.clicked_segment_idx = Some(i);
                                interaction.new_selected_region = Some((blk[i], blk[i + 1]));
                                break;
                            }
                        }
                        interaction.seek_to_seconds = Some(target_time);
                    }
                }
                // In Pointer mode:
                else if let Some(m_idx) = hovered_marker_idx {
                    *selected_marker_idx = Some(m_idx);
                    if response.dragged() {
                        interaction.dragged_marker = Some((m_idx, target_time));
                    }
                } else if response.dragged() {
                    // Dragging in waveform without hovering a marker selects the section for the Lupe!
                    if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                        let t_start = screen_x_to_time(origin.x);
                        let t_curr = target_time;
                        if (t_curr - t_start).abs() > 0.05 {
                            interaction.new_selected_region = Some((t_start.min(t_curr), t_start.max(t_curr)));
                        }
                    }
                } else if response.clicked() {
                    // Regular click in waveform sets playhead and selects segment
                    interaction.seek_to_seconds = Some(target_time);
                    let mut blk = vec![0.0];
                    for &m in markers {
                        if m > 0.0 && m < total_duration_sec {
                            blk.push(m);
                        }
                    }
                    blk.push(total_duration_sec);
                    for i in 0..blk.len() - 1 {
                        if target_time >= blk[i] && target_time < blk[i + 1] {
                            interaction.clicked_segment_idx = Some(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    // Right-click on a marker deletes it
    if response.secondary_clicked() {
        if let Some(m_idx) = hovered_marker_idx {
            interaction.delete_marker_idx = Some(m_idx);
        }
    }

    interaction
}

fn draw_track_header(painter: &egui::Painter, rect: Rect, ch_badge: &str, title: &str, accent_col: Color32) {
    draw_recessed_well(painter, rect, 4.0, BG_INSET);

    let badge_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.min.y + 20.0),
        Vec2::new(28.0, 20.0),
    );
    painter.rect_filled(badge_rect, 4.0, accent_col.gamma_multiply(0.2));
    painter.rect_stroke(badge_rect, 4.0, Stroke::new(1.0, accent_col));
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        ch_badge,
        egui::FontId::proportional(12.0),
        accent_col,
    );

    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 44.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::monospace(9.0),
        TEXT_MUTED,
    );
}

fn draw_track_badge(painter: &egui::Painter, rect: Rect, ch_badge: &str, accent_col: Color32) {
    draw_recessed_well(painter, rect, 4.0, BG_INSET);
    let badge_rect = Rect::from_center_size(
        rect.center(),
        Vec2::new(26.0, 22.0),
    );
    painter.rect_filled(badge_rect, 4.0, accent_col.gamma_multiply(0.2));
    painter.rect_stroke(badge_rect, 4.0, Stroke::new(1.0, accent_col));
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        ch_badge,
        egui::FontId::proportional(12.0),
        accent_col,
    );
}

pub fn render_live_recording_waveform(
    ui: &mut Ui,
    waveform_data: &[StereoWaveformPoint],
    elapsed_sec: f64,
    timer_limit_sec: Option<f64>,
    is_recording: bool,
    idle_text: &str,
) {
    let desired_height = 140.0;
    let available_width = ui.available_width().max(380.0);
    let size = Vec2::new(available_width, desired_height);

    let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();

    // Studio Rack Plate Housing
    painter.rect_filled(rect, 8.0, BG_CARD);
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, BORDER_DARK));

    let header_width = 46.0;
    let wf_left = rect.min.x + header_width + 8.0;
    let wf_right = rect.max.x - 12.0;
    let wf_width = (wf_right - wf_left).max(10.0);

    let wf_top = rect.min.y + 10.0;
    let wf_bottom = wf_top + 92.0;
    let wf_rect = Rect::from_min_max(Pos2::new(wf_left, wf_top), Pos2::new(wf_right, wf_bottom));

    let ruler_top = wf_bottom + 4.0;
    let ruler_bottom = ruler_top + 22.0;
    let ruler_rect = Rect::from_min_max(Pos2::new(wf_left, ruler_top), Pos2::new(wf_right, ruler_bottom));

    // Two Stereo Lanes
    let lane_gap = 4.0;
    let lane_h = (wf_rect.height() - lane_gap) * 0.5;

    let lane_l_top = wf_rect.min.y;
    let lane_l_bot = lane_l_top + lane_h;
    let lane_l_center = lane_l_top + lane_h * 0.5;
    let lane_l_rect = Rect::from_min_max(Pos2::new(wf_rect.min.x, lane_l_top), Pos2::new(wf_rect.max.x, lane_l_bot));

    let lane_r_top = lane_l_bot + lane_gap;
    let lane_r_bot = lane_r_top + lane_h;
    let lane_r_center = lane_r_top + lane_h * 0.5;
    let lane_r_rect = Rect::from_min_max(Pos2::new(wf_rect.min.x, lane_r_top), Pos2::new(wf_rect.max.x, lane_r_bot));

    // Recessed track wells
    draw_recessed_well(painter, lane_l_rect, 4.0, BG_INSET);
    draw_recessed_well(painter, lane_r_rect, 4.0, BG_INSET);

    // Track Header Badges on Left
    let header_l = Rect::from_min_max(Pos2::new(rect.min.x + 8.0, lane_l_top), Pos2::new(rect.min.x + header_width, lane_l_bot));
    draw_track_badge(painter, header_l, "L", WAVE_LEFT);
    let header_r = Rect::from_min_max(Pos2::new(rect.min.x + 8.0, lane_r_top), Pos2::new(rect.min.x + header_width, lane_r_bot));
    draw_track_badge(painter, header_r, "R", WAVE_RIGHT);

    // Center baselines
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_l_center), Pos2::new(wf_rect.max.x, lane_l_center)],
        Stroke::new(1.0, Color32::from_rgb(38, 42, 52)),
    );
    painter.line_segment(
        [Pos2::new(wf_rect.min.x, lane_r_center), Pos2::new(wf_rect.max.x, lane_r_center)],
        Stroke::new(1.0, Color32::from_rgb(38, 42, 52)),
    );

    // Timeline Ruler well
    draw_recessed_well(painter, ruler_rect, 3.0, Color32::from_rgb(26, 27, 32));

    let n_points = waveform_data.len();

    if n_points == 0 || (!is_recording && elapsed_sec <= 0.0) {
        painter.text(
            wf_rect.center(),
            egui::Align2::CENTER_CENTER,
            idle_text,
            egui::FontId::proportional(13.0),
            TEXT_MUTED,
        );
        return;
    }

    // Determine the full time scale
    let total_span_sec = if let Some(limit) = timer_limit_sec {
        if limit > 0.0 {
            limit.max(elapsed_sec)
        } else {
            elapsed_sec.max(10.0)
        }
    } else {
        elapsed_sec.max(10.0)
    };

    // Calculate playhead position
    let playhead_frac = (elapsed_sec / total_span_sec).clamp(0.0, 1.0) as f32;
    let playhead_x = wf_rect.min.x + playhead_frac * wf_width;

    // Render Waveform columns up to playhead_x
    let recorded_width = (playhead_x - wf_rect.min.x).max(1.0);
    let bar_step = (recorded_width / n_points as f32).clamp(1.0, 4.0);
    let columns = (recorded_width / bar_step).floor() as usize;

    if columns > 0 && n_points > 0 {
        for col in 0..columns {
            let frac = col as f32 / columns as f32;
            let pt_idx = ((frac * n_points as f32).floor() as usize).min(n_points - 1);
            let pt = &waveform_data[pt_idx];
            let x = wf_rect.min.x + col as f32 * bar_step;

            // Lane L
            let top_l = lane_l_center - (pt.max_l.clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_l = lane_l_center - (pt.min_l.clamp(-1.0, 0.0) * (lane_h * 0.46));
            painter.line_segment(
                [Pos2::new(x, top_l), Pos2::new(x, bot_l.max(top_l + 1.0))],
                Stroke::new(bar_step.max(1.0), WAVE_LEFT),
            );

            // Lane R
            let top_r = lane_r_center - (pt.max_r.clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_r = lane_r_center - (pt.min_r.clamp(-1.0, 0.0) * (lane_h * 0.46));
            painter.line_segment(
                [Pos2::new(x, top_r), Pos2::new(x, bot_r.max(top_r + 1.0))],
                Stroke::new(bar_step.max(1.0), WAVE_RIGHT),
            );
        }
    }

    // Glowing REC Playhead
    if is_recording {
        let rec_col = Color32::from_rgb(239, 68, 68);
        painter.line_segment(
            [Pos2::new(playhead_x, wf_rect.min.y), Pos2::new(playhead_x, ruler_rect.max.y)],
            Stroke::new(2.0, rec_col),
        );

        // Glowing red recording head top pill
        let pill_rect = Rect::from_center_size(
            Pos2::new(playhead_x, wf_rect.min.y + 4.0),
            Vec2::new(10.0, 8.0),
        );
        painter.rect_filled(pill_rect, 2.0, rec_col);
    }

    // Ruler Ticks
    let ruler_y = ruler_rect.min.y + 11.0;
    let step_sec = if total_span_sec < 60.0 {
        10.0
    } else if total_span_sec < 300.0 {
        30.0
    } else if total_span_sec < 1200.0 {
        120.0
    } else if total_span_sec < 2700.0 {
        300.0
    } else {
        600.0
    };

    let mut t = 0.0;
    while t <= total_span_sec {
        let frac = (t / total_span_sec) as f32;
        let x = wf_rect.min.x + frac * wf_width;
        if x <= wf_rect.max.x {
            painter.line_segment(
                [Pos2::new(x, ruler_rect.min.y + 1.0), Pos2::new(x, ruler_rect.min.y + 4.0)],
                Stroke::new(1.0, Color32::from_rgb(80, 85, 100)),
            );
            let mins = (t / 60.0).floor() as u64;
            let secs = (t % 60.0).floor() as u64;
            painter.text(
                Pos2::new(x, ruler_y),
                egui::Align2::CENTER_CENTER,
                format!("{:02}:{:02}", mins, secs),
                egui::FontId::monospace(9.0),
                TEXT_MUTED,
            );
        }
        t += step_sec;
    }
}

#[derive(Default)]
pub struct ScopeInteraction {
    pub clicked_transient_time: Option<f64>,
    pub seek_to_time: Option<f64>,
    pub dragged_marker: Option<(usize, f64)>,
    pub selected_marker_idx: Option<usize>,
}

pub fn render_detail_magnifier_scope(
    ui: &mut Ui,
    waveform_data: &[StereoWaveformPoint],
    samples_l: &[f32],
    samples_r: &[f32],
    sample_rate: u32,
    total_duration_sec: f64,
    region: (f64, f64),
    markers: &[f64],
    selected_marker_idx: Option<usize>,
    vertical_zoom: f32,
    transients: &[crate::splitter::TransientPoint],
    show_transients: bool,
    scope_zoom: f32,
    scope_scroll_frac: &mut f32,
    current_playhead_sec: f64,
) -> ScopeInteraction {
    let mut interaction = ScopeInteraction::default();

    let scope_h = 140.0;
    let available_w = ui.available_width().max(380.0);
    let size = Vec2::new(available_w, scope_h);

    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter();

    // Recessed dark oscilloscope plate
    painter.rect_filled(rect, 6.0, Color32::from_rgb(14, 16, 22));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgb(45, 52, 70)));

    let header_w = 44.0;
    let scope_left = rect.min.x + header_w + 6.0;
    let scope_right = rect.max.x - 12.0;
    let scope_w = (scope_right - scope_left).max(10.0);

    // Mouse wheel pan within scope plate
    if resp.hovered() {
        let scroll_delta = ui.input(|i| i.raw_scroll_delta);
        if scroll_delta.x.abs() > 0.1 || scroll_delta.y.abs() > 0.1 {
            let delta = if scroll_delta.x.abs() > 0.1 { scroll_delta.x } else { scroll_delta.y };
            let step = (delta / scope_w) * 0.6;
            *scope_scroll_frac = (*scope_scroll_frac - step).clamp(0.0, 1.0);
        }
    }

    let base_start_t = region.0.max(0.0);
    let base_end_t = region.1.min(total_duration_sec).max(base_start_t + 0.01);
    let base_span_sec = (base_end_t - base_start_t).max(0.01);

    let zoom = (scope_zoom as f64).clamp(1.0, 100.0);
    let window_span_sec = (base_span_sec / zoom).max(0.001);
    let max_scroll_span = (base_span_sec - window_span_sec).max(0.0);
    let start_t = base_start_t + (*scope_scroll_frac as f64).clamp(0.0, 1.0) * max_scroll_span;
    let end_t = start_t + window_span_sec;

    let scope_top = rect.min.y + 6.0;
    let scope_wf_h = 92.0;
    let scope_bottom = scope_top + scope_wf_h;
    let scope_rect = Rect::from_min_max(Pos2::new(scope_left, scope_top), Pos2::new(scope_right, scope_bottom));

    let ruler_top = scope_bottom + 2.0;
    let ruler_bottom = ruler_top + 18.0;
    let ruler_rect = Rect::from_min_max(Pos2::new(scope_left, ruler_top), Pos2::new(scope_right, ruler_bottom));

    // Stereo lanes inside scope
    let lane_gap = 4.0;
    let lane_h = (scope_rect.height() - lane_gap) * 0.5;

    let lane_l_top = scope_rect.min.y;
    let lane_l_bot = lane_l_top + lane_h;
    let lane_l_center = lane_l_top + lane_h * 0.5;

    let lane_r_top = lane_l_bot + lane_gap;
    let lane_r_bot = lane_r_top + lane_h;
    let lane_r_center = lane_r_top + lane_h * 0.5;

    // Badges on left
    draw_track_badge(painter, Rect::from_min_max(Pos2::new(rect.min.x + 6.0, lane_l_top), Pos2::new(rect.min.x + header_w, lane_l_bot)), "L", WAVE_LEFT);
    draw_track_badge(painter, Rect::from_min_max(Pos2::new(rect.min.x + 6.0, lane_r_top), Pos2::new(rect.min.x + header_w, lane_r_bot)), "R", WAVE_RIGHT);

    // Center zero lines
    painter.line_segment(
        [Pos2::new(scope_left, lane_l_center), Pos2::new(scope_right, lane_l_center)],
        Stroke::new(1.0, Color32::from_rgb(32, 38, 50)),
    );
    painter.line_segment(
        [Pos2::new(scope_left, lane_r_center), Pos2::new(scope_right, lane_r_center)],
        Stroke::new(1.0, Color32::from_rgb(32, 38, 50)),
    );

    let t_to_x = |t: f64| -> f32 {
        let frac = ((t - start_t) / window_span_sec).clamp(0.0, 1.0) as f32;
        scope_left + frac * scope_w
    };

    let x_to_t = |x: f32| -> f64 {
        let frac = ((x - scope_left) / scope_w).clamp(0.0, 1.0) as f64;
        start_t + frac * window_span_sec
    };

    // Adaptive Grid lines
    let (step_sub, step_major) = if window_span_sec <= 0.1 {
        (0.005, 0.02)
    } else if window_span_sec <= 0.5 {
        (0.01, 0.05)
    } else if window_span_sec <= 2.0 {
        (0.05, 0.25)
    } else if window_span_sec <= 8.0 {
        (0.1, 0.5)
    } else if window_span_sec <= 25.0 {
        (0.5, 2.0)
    } else {
        (1.0, 5.0)
    };

    let mut grid_t = (start_t / step_sub).floor() * step_sub;
    while grid_t <= end_t {
        let gx = t_to_x(grid_t);
        let is_major = (grid_t / step_major).round() * step_major == grid_t
            || ((grid_t / step_major) - (grid_t / step_major).round()).abs() < 0.0001;

        let grid_stroke = if is_major {
            Stroke::new(1.0, Color32::from_rgb(42, 48, 64))
        } else {
            Stroke::new(0.5, Color32::from_rgb(24, 28, 38))
        };
        painter.line_segment([Pos2::new(gx, scope_rect.min.y), Pos2::new(gx, scope_rect.max.y)], grid_stroke);

        if is_major {
            painter.line_segment([Pos2::new(gx, ruler_rect.min.y), Pos2::new(gx, ruler_rect.min.y + 4.0)], Stroke::new(1.0, Color32::from_rgb(90, 100, 120)));
            let m = (grid_t / 60.0).floor() as u64;
            let s = grid_t % 60.0;
            let label = if window_span_sec <= 1.0 {
                format!("{:02}:{:06.3}", m, s)
            } else if window_span_sec <= 5.0 {
                format!("{:02}:{:05.2}", m, s)
            } else {
                format!("{:02}:{:04.1}", m, s)
            };
            painter.text(
                Pos2::new(gx, ruler_rect.min.y + 6.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::monospace(8.5),
                Color32::from_rgb(130, 140, 160),
            );
        }
        grid_t += step_sub;
    }

    let amp_scale = (vertical_zoom * 1.5).clamp(1.0, 32.0);

    // High-Resolution Waveform rendering
    let has_raw_samples = !samples_l.is_empty() && sample_rate > 0;
    if has_raw_samples {
        let sr = sample_rate as f64;
        let num_cols = (scope_w as usize).max(1);
        let col_w = (scope_w / num_cols as f32).max(1.0);

        for c in 0..num_cols {
            let x = scope_left + c as f32 * col_w;
            let pt0 = start_t + (c as f64 / num_cols as f64) * window_span_sec;
            let pt1 = start_t + ((c + 1) as f64 / num_cols as f64) * window_span_sec;

            let s0 = ((pt0 * sr).max(0.0) as usize).min(samples_l.len());
            let s1 = ((pt1 * sr).ceil() as usize).min(samples_l.len()).max(s0 + 1);

            let mut min_l = 0.0_f32;
            let mut max_l = 0.0_f32;
            let mut min_r = 0.0_f32;
            let mut max_r = 0.0_f32;

            for i in s0..s1 {
                let sl = samples_l[i];
                let sr = samples_r[i];
                if sl < min_l { min_l = sl; }
                if sl > max_l { max_l = sl; }
                if sr < min_r { min_r = sr; }
                if sr > max_r { max_r = sr; }
            }

            let top_l = lane_l_center - ((max_l * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.47));
            let bot_l = lane_l_center - ((min_l * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.47));
            let h_l = (bot_l - top_l).max(1.0);
            painter.line_segment([Pos2::new(x, top_l), Pos2::new(x, top_l + h_l)], Stroke::new(col_w, WAVE_LEFT));

            let top_r = lane_r_center - ((max_r * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.47));
            let bot_r = lane_r_center - ((min_r * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.47));
            let h_r = (bot_r - top_r).max(1.0);
            painter.line_segment([Pos2::new(x, top_r), Pos2::new(x, top_r + h_r)], Stroke::new(col_w, WAVE_RIGHT));
        }
    } else {
        // Fallback to waveform cache
        let n_pts = waveform_data.len();
        if n_pts > 0 && total_duration_sec > 0.0 {
            let pts_per_sec = n_pts as f64 / total_duration_sec;
            let start_idx = ((start_t * pts_per_sec).floor() as usize).min(n_pts);
            let end_idx = ((end_t * pts_per_sec).ceil() as usize).min(n_pts).max(start_idx + 1);
            let visible_count = end_idx - start_idx;
            let col_w = (scope_w / visible_count as f32).clamp(1.0, 6.0);

            for (rel, p_idx) in (start_idx..end_idx).enumerate() {
                let pt = &waveform_data[p_idx];
                let x = scope_left + (rel as f32 / visible_count as f32) * scope_w;

                let top_l = lane_l_center - ((pt.max_l * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.47));
                let bot_l = lane_l_center - ((pt.min_l * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.47));
                let h_l = (bot_l - top_l).max(1.0);
                painter.line_segment([Pos2::new(x, top_l), Pos2::new(x, top_l + h_l)], Stroke::new(col_w, WAVE_LEFT));

                let top_r = lane_r_center - ((pt.max_r * amp_scale).clamp(0.0, 1.0) * (lane_h * 0.47));
                let bot_r = lane_r_center - ((pt.min_r * amp_scale).clamp(-1.0, 0.0) * (lane_h * 0.47));
                let h_r = (bot_r - top_r).max(1.0);
                painter.line_segment([Pos2::new(x, top_r), Pos2::new(x, top_r + h_r)], Stroke::new(col_w, WAVE_RIGHT));
            }
        }
    }

    let mouse_pos = resp.hover_pos();

    // Render Transient Spikes & Diamond Markers (if enabled)
    if show_transients {
        for tp in transients {
            if tp.time_sec >= start_t && tp.time_sec <= end_t {
                let tx = t_to_x(tp.time_sec);
                let is_hovered = mouse_pos.map(|p| (p.x - tx).abs() < 6.0 && scope_rect.contains(p)).unwrap_or(false);

                let t_col = if is_hovered {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(255, 215, 0)
                };

                // Vertical transient spike
                painter.line_segment(
                    [Pos2::new(tx, scope_rect.min.y), Pos2::new(tx, scope_rect.max.y)],
                    Stroke::new(if is_hovered { 1.8 } else { 1.2 }, t_col),
                );

                // Diamond head at the top
                let d_pts = vec![
                    Pos2::new(tx, scope_rect.min.y + 1.0),
                    Pos2::new(tx + 4.0, scope_rect.min.y + 6.0),
                    Pos2::new(tx, scope_rect.min.y + 11.0),
                    Pos2::new(tx - 4.0, scope_rect.min.y + 6.0),
                ];
                painter.add(egui::Shape::convex_polygon(d_pts, t_col, Stroke::NONE));

                if is_hovered {
                    painter.circle_stroke(Pos2::new(tx, scope_rect.min.y + 6.0), 7.0, Stroke::new(1.5, Color32::WHITE));
                    let tm = (tp.time_sec / 60.0).floor() as u64;
                    let ts = tp.time_sec % 60.0;
                    let tip_tag = Rect::from_center_size(Pos2::new(tx, scope_rect.min.y + 24.0), Vec2::new(110.0, 18.0));
                    painter.rect_filled(tip_tag, 3.0, Color32::from_rgb(28, 24, 10));
                    painter.rect_stroke(tip_tag, 3.0, Stroke::new(1.0, Color32::from_rgb(255, 215, 0)));
                    painter.text(tip_tag.center(), egui::Align2::CENTER_CENTER, format!("⚡ {:02}:{:05.2}s", tm, ts), egui::FontId::monospace(9.0), Color32::from_rgb(255, 215, 0));

                    if resp.clicked() {
                        interaction.clicked_transient_time = Some(tp.time_sec);
                    }
                }
            }
        }
    }

    // Existing Markers in view with Dragging and Nudging support
    let mut hovered_scope_marker = None;
    for (m_i, &m) in markers.iter().enumerate() {
        if m >= start_t && m <= end_t {
            let mx = t_to_x(m);
            let is_sel = selected_marker_idx == Some(m_i);
            let col = if is_sel { Color32::from_rgb(255, 215, 0) } else { LED_AMBER };
            painter.line_segment([Pos2::new(mx, scope_rect.min.y), Pos2::new(mx, ruler_rect.max.y)], Stroke::new(2.0, col));

            let flag = Rect::from_center_size(Pos2::new(mx, scope_rect.min.y + 10.0), Vec2::new(36.0, 18.0));
            painter.rect_filled(flag, 3.0, col);
            painter.rect_stroke(flag, 3.0, Stroke::new(1.0, BORDER_DARK));
            painter.text(flag.center(), egui::Align2::CENTER_CENTER, format!("T{:02}", m_i + 2), egui::FontId::monospace(9.5), Color32::BLACK);

            if let Some(pos) = mouse_pos {
                if (pos.x - mx).abs() < 10.0 && (scope_rect.contains(pos) || ruler_rect.contains(pos)) {
                    hovered_scope_marker = Some((m_i, m));
                }
            }
        }
    }

    // Playhead indicator
    if current_playhead_sec >= start_t && current_playhead_sec <= end_t {
        let px = t_to_x(current_playhead_sec);
        painter.line_segment(
            [Pos2::new(px, scope_rect.min.y), Pos2::new(px, ruler_rect.max.y)],
            Stroke::new(2.0, Color32::WHITE),
        );
        let tri = vec![
            Pos2::new(px - 5.0, ruler_rect.min.y),
            Pos2::new(px + 5.0, ruler_rect.min.y),
            Pos2::new(px, ruler_rect.min.y + 8.0),
        ];
        painter.add(egui::Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
    }

    // Direct Scope Interactions (Drag marker, select marker, or seek)
    if let Some((m_idx, _)) = hovered_scope_marker {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        if resp.clicked() {
            interaction.selected_marker_idx = Some(m_idx);
        }
        if resp.dragged() {
            if let Some(pos) = mouse_pos {
                let new_t = x_to_t(pos.x);
                interaction.selected_marker_idx = Some(m_idx);
                interaction.dragged_marker = Some((m_idx, new_t));
            }
        }
    } else if resp.clicked() {
        if let Some(pos) = mouse_pos {
            if scope_rect.contains(pos) || ruler_rect.contains(pos) {
                let clicked_t = x_to_t(pos.x);
                if interaction.clicked_transient_time.is_none() {
                    interaction.seek_to_time = Some(clicked_t);
                }
            }
        }
    }

    interaction
}

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
}

pub struct WaveformInteraction {
    pub seek_to_seconds: Option<f64>,
    pub add_marker_at_seconds: Option<f64>,
    pub dragged_marker: Option<(usize, f64)>,
    pub delete_marker_idx: Option<usize>,
    pub hovered_time_sec: Option<f64>,
    pub mouse_pos_in_waveform: Option<Pos2>,
    pub is_mouse_over_waveform: bool,
}

pub fn render_stereo_waveform(
    ui: &mut Ui,
    waveform_data: &[StereoWaveformPoint],
    total_duration_sec: f64,
    current_playhead_sec: f64,
    markers: &[f64],
    selected_marker_idx: &mut Option<usize>,
    zoom_factor: &mut f32,
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
    };

    let desired_height = 230.0;
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

    // Height breakdown:
    // Waveform lanes: top 10.0 to 166.0 (height 156.0: 75.0 + 6.0 gap + 75.0)
    // Timeline Ruler: 172.0 to 196.0 (height 24.0)
    // Overview Bar:   202.0 to 220.0 (height 18.0)
    let wf_top = rect.min.y + 10.0;
    let wf_bottom = wf_top + 154.0;
    let wf_rect = Rect::from_min_max(Pos2::new(wf_left, wf_top), Pos2::new(wf_right, wf_bottom));

    let ruler_top = wf_bottom + 4.0;
    let ruler_bottom = ruler_top + 24.0;
    let ruler_rect = Rect::from_min_max(Pos2::new(wf_left, ruler_top), Pos2::new(wf_right, ruler_bottom));

    let overview_top = ruler_bottom + 4.0;
    let overview_bottom = overview_top + 18.0;
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
    // Render Dual Waveforms
    // -------------------------------------------------------------
    let n_points = waveform_data.len();
    if n_points > 0 {
        let start_pt = ((view_start_frac * n_points as f64).floor() as usize).min(n_points);
        let end_pt = ((view_end_frac * n_points as f64).ceil() as usize).min(n_points);
        let visible_count = (end_pt - start_pt).max(1);
        let bar_w = (wf_width / visible_count as f32).clamp(1.0, 4.0);

        for (rel_idx, pt_idx) in (start_pt..end_pt).enumerate() {
            let pt = &waveform_data[pt_idx];
            let x = wf_rect.min.x + (rel_idx as f32 / visible_count as f32) * wf_width;

            // Lane L
            let top_l = lane_l_center - (pt.max_l.clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_l = lane_l_center - (pt.min_l.clamp(-1.0, 0.0) * (lane_h * 0.46));
            painter.line_segment(
                [Pos2::new(x, top_l), Pos2::new(x, bot_l.max(top_l + 1.0))],
                Stroke::new(bar_w, WAVE_LEFT),
            );

            // Lane R
            let top_r = lane_r_center - (pt.max_r.clamp(0.0, 1.0) * (lane_h * 0.46));
            let bot_r = lane_r_center - (pt.min_r.clamp(-1.0, 0.0) * (lane_h * 0.46));
            painter.line_segment(
                [Pos2::new(x, top_r), Pos2::new(x, bot_r.max(top_r + 1.0))],
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

    // Overview mini waveform representation or markers
    let ov_w = overview_rect.width();
    for &m in markers {
        if total_duration_sec > 0.0 {
            let m_frac = (m / total_duration_sec).clamp(0.0, 1.0);
            let mx = overview_rect.min.x + (m_frac as f32) * ov_w;
            painter.line_segment(
                [Pos2::new(mx, overview_rect.min.y + 2.0), Pos2::new(mx, overview_rect.max.y - 2.0)],
                Stroke::new(1.0, LED_AMBER),
            );
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
    // 3. Waveform in Pointer Mode: drag marker handle or seek playhead
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
                // In Scissor mode (or Shift/Cmd held): click places a marker
                if is_scissor {
                    if response.clicked() {
                        interaction.add_marker_at_seconds = Some(target_time);
                    }
                }
                // In Pointer mode:
                else if let Some(m_idx) = hovered_marker_idx {
                    *selected_marker_idx = Some(m_idx);
                    if response.dragged() {
                        interaction.dragged_marker = Some((m_idx, target_time));
                    }
                } else if response.clicked() {
                    // Regular click in waveform sets playhead
                    interaction.seek_to_seconds = Some(target_time);
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

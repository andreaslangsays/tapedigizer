use egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, Ui, Vec2};

use crate::ui::theme::{palette, RADIUS_CONTROL};

pub(super) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |c0: u8, c1: u8| -> u8 { (c0 as f32 + (c1 as f32 - c0 as f32) * t).round() as u8 };
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

/// Zeichnet ein abgerundetes Rechteck mit weichem vertikalem Farbverlauf per GPU-Mesh.
pub fn paint_rounded_vertical_gradient(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
) {
    if rect.width() <= 0.5 || rect.height() <= 0.5 {
        return;
    }
    let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    let mut ring: Vec<Pos2> = Vec::with_capacity(28);
    if r <= 0.5 {
        ring.push(rect.left_top());
        ring.push(rect.right_top());
        ring.push(rect.right_bottom());
        ring.push(rect.left_bottom());
    } else {
        let steps = 5;
        let corners = [
            (
                Pos2::new(rect.max.x - r, rect.min.y + r),
                -std::f32::consts::FRAC_PI_2,
                0.0,
            ),
            (
                Pos2::new(rect.max.x - r, rect.max.y - r),
                0.0,
                std::f32::consts::FRAC_PI_2,
            ),
            (
                Pos2::new(rect.min.x + r, rect.max.y - r),
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
            ),
            (
                Pos2::new(rect.min.x + r, rect.min.y + r),
                std::f32::consts::PI,
                std::f32::consts::PI * 1.5,
            ),
        ];
        for (center, start_a, end_a) in corners {
            for i in 0..=steps {
                let a = start_a + (end_a - start_a) * (i as f32 / steps as f32);
                ring.push(center + Vec2::new(a.cos(), a.sin()) * r);
            }
        }
    }

    let mut mesh = Mesh::default();
    let c = rect.center();
    let color_at = |y: f32| {
        let t = ((y - rect.min.y) / rect.height().max(1.0)).clamp(0.0, 1.0);
        lerp_color(top, bottom, t)
    };
    mesh.colored_vertex(c, color_at(c.y));
    for &p in &ring {
        mesh.colored_vertex(p, color_at(p.y));
    }
    let count = ring.len() as u32;
    for i in 0..count {
        let next = (i + 1) % count;
        mesh.add_triangle(0, 1 + i, 1 + next);
    }
    painter.add(Shape::mesh(mesh));
}

/// Glatte, seidenmatte Aluminium-/Farb-Fläche mit sehr leichtem vertikalem Verlauf.
pub fn paint_bevel_surface(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
    pressed: bool,
    stroke_col: Option<Color32>,
) {
    let p = palette(painter.ctx());
    if !pressed {
        let shadow_alpha = if p.is_dark { 52 } else { 18 };
        painter.rect_filled(
            rect.translate(Vec2::new(0.0, 1.0)),
            radius,
            Color32::from_rgba_unmultiplied(0, 0, 0, shadow_alpha),
        );
    }

    paint_rounded_vertical_gradient(painter, rect, radius, top, bottom);

    if let Some(col) = stroke_col {
        painter.rect_stroke(rect.shrink(0.5), radius, Stroke::new(0.75, col));
    }
}

/// Vertiefte Display-Fläche mit Kontur aus dem aktiven Theme.
pub fn draw_recessed_well(painter: &egui::Painter, rect: Rect, rounding: f32, fill: Color32) {
    let p = palette(painter.ctx());
    painter.rect_filled(rect, rounding, fill);
    painter.rect_stroke(
        rect.shrink(0.5),
        rounding,
        Stroke::new(0.75, p.border_subtle),
    );
}

pub fn draw_keyboard_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        let p = palette(ui.ctx());
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            RADIUS_CONTROL + 1.0,
            Stroke::new(2.0, p.focus),
        );
    }
}

use egui::{Color32, Pos2, Rect, Stroke};

// Studio & Logic Pro Hardware Palette
// Matches the macOS dark neutral titlebar chrome (#252528)
pub const BG_WINDOW: Color32 = Color32::from_rgb(37, 37, 40);
pub const BG_PANEL: Color32 = Color32::from_rgb(47, 47, 52);
pub const BG_CARD: Color32 = Color32::from_rgb(42, 42, 47);
pub const BG_INSET: Color32 = Color32::from_rgb(23, 23, 26);
pub const BG_LCD: Color32 = Color32::from_rgb(17, 19, 23);

pub const BORDER_DARK: Color32 = Color32::from_rgb(28, 28, 32);
pub const BORDER_MID: Color32 = Color32::from_rgb(58, 58, 66);
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(76, 76, 86);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 242, 246);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 165, 178);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(115, 120, 132);

// Hardware LED & Signal Colors
pub const LED_GREEN: Color32 = Color32::from_rgb(46, 204, 113);
pub const LED_GREEN_DIM: Color32 = Color32::from_rgb(24, 52, 36);
pub const LED_AMBER: Color32 = Color32::from_rgb(243, 156, 18);
pub const LED_AMBER_DIM: Color32 = Color32::from_rgb(60, 42, 18);
pub const LED_RED: Color32 = Color32::from_rgb(231, 76, 60);
pub const LED_RED_DIM: Color32 = Color32::from_rgb(56, 22, 22);

pub const WAVE_LEFT: Color32 = Color32::from_rgb(56, 189, 248);   // Studio Cyan
pub const WAVE_RIGHT: Color32 = Color32::from_rgb(147, 130, 248); // Studio Lavender

// Draws a tactile hardware panel with an authentic 3D beveled bezel
pub fn draw_tactile_panel(painter: &egui::Painter, rect: Rect, rounding: f32) {
    // Drop shadow
    let shadow_rect = rect.translate(egui::vec2(0.0, 2.0));
    painter.rect_filled(shadow_rect, rounding, Color32::from_rgba_unmultiplied(0, 0, 0, 70));

    // Base face
    painter.rect_filled(rect, rounding, BG_PANEL);

    // Top highlight (reflection of studio ambient ceiling light)
    painter.line_segment(
        [Pos2::new(rect.min.x + rounding, rect.min.y + 1.0), Pos2::new(rect.max.x - rounding, rect.min.y + 1.0)],
        Stroke::new(1.0, BORDER_LIGHT),
    );

    // Outer border
    painter.rect_stroke(rect, rounding, Stroke::new(1.0, BORDER_DARK));
}

// Draws a recessed hardware cavity / well (like an LCD screen or meter groove)
pub fn draw_recessed_well(painter: &egui::Painter, rect: Rect, rounding: f32, fill: Color32) {
    painter.rect_filled(rect, rounding, fill);

    // Inner shadow at top
    painter.line_segment(
        [Pos2::new(rect.min.x + rounding, rect.min.y + 1.0), Pos2::new(rect.max.x - rounding, rect.min.y + 1.0)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 160)),
    );

    // Bottom subtle highlight border
    painter.line_segment(
        [Pos2::new(rect.min.x + rounding, rect.max.y), Pos2::new(rect.max.x - rounding, rect.max.y)],
        Stroke::new(1.0, BORDER_LIGHT.gamma_multiply(0.4)),
    );

    painter.rect_stroke(rect, rounding, Stroke::new(1.0, BORDER_DARK));
}

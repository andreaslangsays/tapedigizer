use egui::{Button, Color32, Frame, Pos2, Rect, RichText, Stroke, Ui, Vec2};

// Studio & Logic Pro Hardware Palette
// Matches the macOS dark neutral titlebar chrome (#252528)
pub const BG_WINDOW: Color32 = Color32::from_rgb(20, 23, 26);
pub const BG_PANEL: Color32 = Color32::from_rgb(31, 35, 39);
pub const BG_CARD: Color32 = Color32::from_rgb(25, 29, 33);
pub const BG_INSET: Color32 = Color32::from_rgb(16, 19, 22);

pub const BORDER_DARK: Color32 = Color32::from_rgb(28, 28, 32);
pub const BORDER_MID: Color32 = Color32::from_rgb(58, 58, 66);
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(76, 76, 86);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 242, 246);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 165, 178);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(139, 146, 159);

// Hardware LED & Signal Colors
pub const LED_GREEN: Color32 = Color32::from_rgb(46, 204, 113);
pub const LED_GREEN_DIM: Color32 = Color32::from_rgb(24, 52, 36);
pub const LED_AMBER: Color32 = Color32::from_rgb(243, 156, 18);
pub const LED_AMBER_DIM: Color32 = Color32::from_rgb(60, 42, 18);
pub const LED_RED: Color32 = Color32::from_rgb(231, 76, 60);
pub const LED_RED_DIM: Color32 = Color32::from_rgb(56, 22, 22);

pub const WAVE_LEFT: Color32 = Color32::from_rgb(56, 189, 248);   // Studio Cyan
pub const WAVE_RIGHT: Color32 = Color32::from_rgb(147, 130, 248); // Studio Lavender

pub const ACCENT: Color32 = Color32::from_rgb(229, 194, 137);
const ACCENT_DARK: Color32 = Color32::from_rgb(75, 61, 44);

pub fn panel_frame() -> Frame {
    Frame::none().fill(BG_PANEL)
        .stroke(Stroke::new(1.0, BORDER_MID))
        .rounding(10.0)
        .inner_margin(12.0)
}

pub fn panel_title(ui: &mut Ui, title: &str) {
    ui.horizontal(|ui| {
        let (indicator, _) = ui.allocate_exact_size(Vec2::new(3.0, 12.0), egui::Sense::hover());
        ui.painter().rect_filled(indicator, 1.5, ACCENT);
        ui.label(RichText::new(title).size(14.0).strong().color(TEXT_PRIMARY));
    });
}

pub fn studio_wordmark(ui: &mut Ui) {
    let (mark, _) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), egui::Sense::hover());
    ui.painter().circle_stroke(mark.center(), 7.0, Stroke::new(1.2, ACCENT));
    ui.painter().circle_filled(mark.center(), 2.5, ACCENT);
    ui.label(RichText::new("TapeDigizer").size(16.0).strong().color(ACCENT));
}

pub fn studio_panel(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    panel_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        panel_title(ui, title);
        ui.add_space(4.0);
        contents(ui);
    });
}

pub fn primary_button(label: impl Into<egui::WidgetText>) -> Button<'static> {
    let label = label.into().strong().color(BG_INSET);
    Button::new(label).fill(ACCENT).rounding(7.0).min_size(Vec2::new(72.0, 27.0))
}

pub fn record_button(label: &str) -> Button<'_> {
    Button::new(RichText::new(label).strong().color(Color32::WHITE))
        .fill(Color32::from_rgb(150, 48, 48))
        .rounding(7.0).min_size(Vec2::new(100.0, 27.0))
}

pub fn mode_button(label: impl Into<egui::WidgetText>, selected: bool) -> Button<'static> {
    Button::new(label.into()).selected(selected).min_size(Vec2::new(0.0, 25.0))
}

pub fn time_display(ui: &mut Ui, value: String) {
    Frame::none().fill(BG_INSET).rounding(5.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(value).monospace().color(ACCENT));
        });
}

pub fn studio_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_WINDOW;
    visuals.window_fill = BG_PANEL;
    visuals.extreme_bg_color = BG_INSET;
    visuals.window_stroke = Stroke::new(1.0, BORDER_MID);
    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT_PRIMARY;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(53, 57, 64);
    visuals.widgets.inactive.weak_bg_fill = visuals.widgets.inactive.bg_fill;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(65, 70, 79);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    for widget in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
        widget.rounding = egui::Rounding::same(6.0);
        widget.expansion = 0.0;
    }
    visuals
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

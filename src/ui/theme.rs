use egui::{Color32, Stroke, Vec2};

// =============================================================================
// Dual-Theme Engine: Silverface (Light Mode) & Obsidian (Dark Mode)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Silverface, // Light Mode (Silber-Aluminium + Kupfer-Amber + Studio-Petrol)
    Obsidian,   // Dark Mode (Tiefes Graphit + Warm Amber + Phosphor-Grün)
}

pub fn theme_mode(ctx: &egui::Context) -> ThemeMode {
    ctx.data(|data| data.get_temp(theme_id()))
        .unwrap_or(ThemeMode::Silverface)
}

fn theme_id() -> egui::Id {
    egui::Id::new("tapedigizer_theme")
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    let egui_theme = match mode {
        ThemeMode::Silverface => egui::Theme::Light,
        ThemeMode::Obsidian => egui::Theme::Dark,
    };
    ctx.data_mut(|data| data.insert_temp(theme_id(), mode));
    ctx.set_style_of(egui_theme, studio_style_for(palette(ctx)));
    ctx.set_theme(egui_theme);
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub is_dark: bool,
    // Surface Elevation (Level 0..3)
    pub bg_inset: Color32,
    pub bg_window: Color32,
    pub bg_toolbar: Color32,
    pub bg_panel: Color32,
    pub bg_card: Color32,
    pub border_subtle: Color32,
    pub border_mid: Color32,
    // Typography
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_on_accent: Color32,
    pub text_on_bright_accent: Color32,
    // Primary Copper-Amber & Secondary Studio-Teal/Green
    pub accent_top: Color32,
    pub accent_bottom: Color32,
    pub accent_soft: Color32,
    pub teal: Color32,
    pub teal_dim: Color32,
    pub amber: Color32,
    pub amber_dim: Color32,
    pub red: Color32,
    pub red_bottom: Color32,
    pub red_dim: Color32,
    pub focus: Color32,
    // Waveforms
    pub wave_left: Color32,
    pub wave_right: Color32,
    pub edit_highlight: Color32,
    // Controls (glatte Silber- / Graphit-Verläufe)
    pub btn_top: Color32,
    pub btn_bottom: Color32,
    pub btn_hover_top: Color32,
    pub btn_hover_bottom: Color32,
    pub btn_pressed_top: Color32,
    pub btn_pressed_bottom: Color32,
    // Silber-Kappen (Rotaries / Fader)
    pub knob_top: Color32,
    pub knob_bottom: Color32,
    pub knob_cap_top: Color32,
    pub knob_cap_bottom: Color32,
    // Vintage VU Dial
    pub vu_face_top: Color32,
    pub vu_face_bottom: Color32,
    pub vu_scale: Color32,
    pub vu_needle: Color32,
}

impl Palette {
    /// Exakte Tokens aus StitchMCP: "Precision Mastering Console" (Silverface Light)
    pub const fn silverface() -> Self {
        Self {
            is_dark: false,
            bg_inset: Color32::from_rgb(245, 243, 238), // #f5f3ee Data Wells
            bg_window: Color32::from_rgb(229, 226, 220), // #e5e2dc Base Rack Shell (warm silver-gray)
            bg_toolbar: Color32::from_rgb(237, 234, 228), // #edeae4 Milled Faceplate Toolbar
            bg_panel: Color32::from_rgb(249, 248, 246),  // Warm bright faceplate bay
            bg_card: Color32::from_rgb(237, 234, 228),   // #edeae4 Module Faceplates
            border_subtle: Color32::from_rgb(200, 196, 188), // #c8c4bc Milled groove seam
            border_mid: Color32::from_rgb(179, 173, 163), // #b3ada3 Button outline
            text_primary: Color32::from_rgb(18, 20, 23), // #121417 Obsidian Ink
            text_secondary: Color32::from_rgb(76, 74, 70), // Warm dark slate
            text_muted: Color32::from_rgb(112, 108, 102), // Muted slate ink
            text_on_accent: Color32::WHITE,
            text_on_bright_accent: Color32::from_rgb(18, 20, 23),
            accent_top: Color32::from_rgb(212, 106, 32), // Lacquer Orange highlight
            accent_bottom: Color32::from_rgb(189, 86, 18), // #bd5612 Primary Container
            accent_soft: Color32::from_rgb(252, 236, 222), // Soft amber selection tint
            teal: Color32::from_rgb(14, 116, 144),       // #0e7490 Signal Teal
            teal_dim: Color32::from_rgb(214, 228, 232),
            amber: Color32::from_rgb(196, 91, 24), // #c45b18 Signal Amber
            amber_dim: Color32::from_rgb(238, 222, 206),
            red: Color32::from_rgb(186, 26, 26), // #ba1a1a Error / Clip
            red_bottom: Color32::from_rgb(156, 20, 20),
            red_dim: Color32::from_rgb(242, 218, 216),
            focus: Color32::from_rgb(14, 116, 144), // Signal Teal focus outline
            wave_left: Color32::from_rgb(196, 91, 24), // #c45b18 Signal Amber
            wave_right: Color32::from_rgb(14, 116, 144), // #0e7490 Signal Teal
            edit_highlight: Color32::from_rgb(217, 119, 6), // #d97706 Warm Tungsten
            btn_top: Color32::from_rgb(244, 242, 238), // #f4f2ee Bead-blasted silver top
            btn_bottom: Color32::from_rgb(222, 217, 209), // #ded9d1 Bead-blasted silver bottom
            btn_hover_top: Color32::from_rgb(250, 248, 245),
            btn_hover_bottom: Color32::from_rgb(232, 228, 220),
            btn_pressed_top: Color32::from_rgb(212, 207, 199),
            btn_pressed_bottom: Color32::from_rgb(226, 222, 214),
            knob_top: Color32::from_rgb(244, 242, 238), // Milled Aluminum Knob
            knob_bottom: Color32::from_rgb(212, 207, 199),
            knob_cap_top: Color32::from_rgb(250, 249, 246),
            knob_cap_bottom: Color32::from_rgb(228, 224, 217),
            vu_face_top: Color32::from_rgb(251, 248, 238), // #fbf8ee Warm cream parchment
            vu_face_bottom: Color32::from_rgb(240, 234, 214),
            vu_scale: Color32::from_rgb(44, 42, 38),
            vu_needle: Color32::from_rgb(18, 20, 23),
        }
    }

    /// Exakte Tokens aus StitchMCP: "Obsidian Waveform" (Dark Mode)
    pub const fn obsidian() -> Self {
        Self {
            is_dark: true,
            bg_inset: Color32::from_rgb(13, 14, 17), // #0d0e11 Recessed Wells
            bg_window: Color32::from_rgb(18, 19, 22), // #121316 App Shell / Base Layer
            bg_toolbar: Color32::from_rgb(26, 28, 34), // #1a1c22 Deck Surface
            bg_panel: Color32::from_rgb(27, 29, 35), // #1b1d23 Deck Module Frame
            bg_card: Color32::from_rgb(35, 38, 47),  // #23262f Component Faceplate
            border_subtle: Color32::from_rgb(46, 50, 61), // #2e323d Boundary Hairline
            border_mid: Color32::from_rgb(66, 72, 86),
            text_primary: Color32::from_rgb(227, 226, 230), // #e3e2e6 on-surface
            text_secondary: Color32::from_rgb(188, 192, 204),
            text_muted: Color32::from_rgb(138, 145, 160),
            text_on_bright_accent: Color32::from_rgb(42, 23, 0),
            text_on_accent: Color32::from_rgb(42, 23, 0), // #2a1700 on-primary-fixed
            accent_top: Color32::from_rgb(255, 193, 116), // #ffc174 primary
            accent_bottom: Color32::from_rgb(245, 158, 11), // #f59e0b primary-container
            accent_soft: Color32::from_rgb(52, 38, 18),
            teal: Color32::from_rgb(16, 185, 129), // #10b981 Phosphor Green
            teal_dim: Color32::from_rgb(20, 46, 38),
            amber: Color32::from_rgb(245, 158, 11), // #f59e0b Warm Amber
            amber_dim: Color32::from_rgb(54, 38, 16),
            red: Color32::from_rgb(239, 68, 68), // #ef4444 Peak Clip
            red_bottom: Color32::from_rgb(198, 36, 42),
            red_dim: Color32::from_rgb(56, 22, 24),
            focus: Color32::from_rgb(255, 185, 95), // #ffb95f surface-tint
            wave_left: Color32::from_rgb(245, 158, 11), // #f59e0b Warm Amber
            wave_right: Color32::from_rgb(78, 222, 163), // #4edea3 secondary
            edit_highlight: Color32::from_rgb(255, 193, 116),
            btn_top: Color32::from_rgb(48, 52, 62),
            btn_bottom: Color32::from_rgb(35, 38, 47), // #23262f
            btn_hover_top: Color32::from_rgb(58, 63, 75),
            btn_hover_bottom: Color32::from_rgb(44, 48, 59),
            btn_pressed_top: Color32::from_rgb(30, 32, 40),
            btn_pressed_bottom: Color32::from_rgb(38, 42, 51),
            knob_top: Color32::from_rgb(84, 89, 102),
            knob_bottom: Color32::from_rgb(50, 54, 64),
            knob_cap_top: Color32::from_rgb(70, 75, 87),
            knob_cap_bottom: Color32::from_rgb(46, 50, 60),
            vu_face_top: Color32::from_rgb(36, 28, 18),
            vu_face_bottom: Color32::from_rgb(16, 15, 14),
            vu_scale: Color32::from_rgb(168, 156, 138),
            vu_needle: Color32::from_rgb(255, 196, 104),
        }
    }
}

/// Die OLED-Anzeigen verwenden auch in Silverface dunkles Displayglas.
pub const OLED_PALETTE: Palette = Palette::obsidian();

pub fn palette(ctx: &egui::Context) -> Palette {
    match theme_mode(ctx) {
        ThemeMode::Silverface => Palette::silverface(),
        ThemeMode::Obsidian => Palette::obsidian(),
    }
}

// Einheitliche Geometrie-Konstanten
pub const HEIGHT_PRIMARY: f32 = 28.0;
pub const HEIGHT_CONTROL: f32 = 25.0;
pub const HEIGHT_COMPACT: f32 = 22.0;

pub const RADIUS_CONTROL: f32 = 7.0;
pub const RADIUS_WELL: f32 = 8.0;
pub const RADIUS_BAY: f32 = 10.0;

/// Lädt native macOS-Systemschriften (SF Pro & SF Mono), falls auf dem System vorhanden.
pub fn install_macos_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;

    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        fonts
            .font_data
            .insert("macos_sf".into(), egui::FontData::from_owned(bytes));
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "macos_sf".into());
        }
        changed = true;
    }

    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNSMono.ttf") {
        fonts
            .font_data
            .insert("macos_sf_mono".into(), egui::FontData::from_owned(bytes));
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "macos_sf_mono".into());
        }
        changed = true;
    }

    if changed {
        ctx.set_fonts(fonts);
    }
}

fn studio_style_for(palette: Palette) -> egui::Style {
    let mut style = egui::Style::default();
    style.visuals = studio_visuals(palette);
    style.spacing.item_spacing = Vec2::new(6.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 3.0);
    style.spacing.interact_size.y = HEIGHT_CONTROL;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(12.2));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(11.8));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(10.8));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(11.5));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(15.0));
    style
}

fn studio_visuals(p: Palette) -> egui::Visuals {
    let mut visuals = if p.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = p.bg_window;
    visuals.window_fill = p.bg_panel;
    visuals.extreme_bg_color = p.bg_inset;
    visuals.window_stroke = Stroke::new(0.75, p.border_subtle);
    visuals.window_rounding = egui::Rounding::same(RADIUS_BAY);

    visuals.widgets.noninteractive.bg_fill = p.bg_card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.75, p.border_subtle);
    visuals.widgets.noninteractive.fg_stroke.color = p.text_primary;

    visuals.widgets.inactive.bg_fill = p.btn_bottom;
    visuals.widgets.inactive.weak_bg_fill = p.btn_bottom;
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.75, p.border_subtle);
    visuals.widgets.inactive.fg_stroke.color = p.text_primary;

    visuals.widgets.hovered.bg_fill = p.btn_hover_bottom;
    visuals.widgets.hovered.weak_bg_fill = p.btn_hover_bottom;
    visuals.widgets.hovered.bg_stroke = Stroke::new(0.75, p.border_mid);
    visuals.widgets.hovered.fg_stroke.color = p.text_primary;

    visuals.widgets.active.bg_fill = p.accent_soft;
    visuals.widgets.active.weak_bg_fill = p.accent_soft;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, p.focus);
    visuals.widgets.active.fg_stroke.color = p.text_primary;

    visuals.widgets.open.bg_fill = p.btn_hover_bottom;
    visuals.widgets.open.bg_stroke = Stroke::new(1.5, p.focus);
    visuals.widgets.open.fg_stroke.color = p.text_primary;

    visuals.selection.bg_fill = p.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0, p.accent_top);

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.rounding = egui::Rounding::same(RADIUS_CONTROL);
        widget.expansion = 0.0;
    }
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_theme_keeps_other_contexts_in_silverface() {
        let silverface = egui::Context::default();
        let obsidian = egui::Context::default();
        apply_theme(&obsidian, ThemeMode::Obsidian);
        assert_eq!(
            palette(&silverface).bg_window,
            Palette::silverface().bg_window
        );
        assert_eq!(palette(&obsidian).bg_window, Palette::obsidian().bg_window);
        assert_eq!(
            obsidian.style().visuals.panel_fill,
            palette(&obsidian).bg_window
        );
        let _ = obsidian.run(
            egui::RawInput {
                system_theme: Some(egui::Theme::Light),
                ..Default::default()
            },
            |ctx| {
                assert_eq!(ctx.theme(), egui::Theme::Dark);
                assert_eq!(ctx.style().visuals.panel_fill, palette(ctx).bg_window);
            },
        );
    }
}

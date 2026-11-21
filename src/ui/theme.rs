use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Button, Color32, Frame, Mesh, Pos2, Rect, RichText, Shape, Stroke, Ui, Vec2};

// =============================================================================
// Dual-Theme Engine: Silverface (Light Mode) & Obsidian (Dark Mode)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Silverface, // Light Mode (Silber-Aluminium + Kupfer-Amber + Studio-Petrol)
    Obsidian,   // Dark Mode (Tiefes Graphit + Warm Amber + Phosphor-Grün)
}

static DARK_MODE: AtomicBool = AtomicBool::new(false);

pub fn theme_mode() -> ThemeMode {
    if DARK_MODE.load(Ordering::Relaxed) {
        ThemeMode::Obsidian
    } else {
        ThemeMode::Silverface
    }
}

pub fn set_theme_mode(mode: ThemeMode) {
    DARK_MODE.store(mode == ThemeMode::Obsidian, Ordering::Relaxed);
}

#[allow(dead_code)]
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
    pub knob_pointer: Color32,
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
            bg_inset: Color32::from_rgb(245, 243, 238),       // #f5f3ee Data Wells
            bg_window: Color32::from_rgb(229, 226, 220),      // #e5e2dc Base Rack Shell (warm silver-gray)
            bg_toolbar: Color32::from_rgb(237, 234, 228),     // #edeae4 Milled Faceplate Toolbar
            bg_panel: Color32::from_rgb(249, 248, 246),       // Warm bright faceplate bay
            bg_card: Color32::from_rgb(237, 234, 228),        // #edeae4 Module Faceplates
            border_subtle: Color32::from_rgb(200, 196, 188),  // #c8c4bc Milled groove seam
            border_mid: Color32::from_rgb(179, 173, 163),     // #b3ada3 Button outline
            text_primary: Color32::from_rgb(18, 20, 23),      // #121417 Obsidian Ink
            text_secondary: Color32::from_rgb(76, 74, 70),    // Warm dark slate
            text_muted: Color32::from_rgb(112, 108, 102),     // Muted slate ink
            text_on_accent: Color32::WHITE,
            accent_top: Color32::from_rgb(212, 106, 32),      // Lacquer Orange highlight
            accent_bottom: Color32::from_rgb(189, 86, 18),    // #bd5612 Primary Container
            accent_soft: Color32::from_rgb(252, 236, 222),    // Soft amber selection tint
            teal: Color32::from_rgb(14, 116, 144),            // #0e7490 Signal Teal
            teal_dim: Color32::from_rgb(214, 228, 232),
            amber: Color32::from_rgb(196, 91, 24),            // #c45b18 Signal Amber
            amber_dim: Color32::from_rgb(238, 222, 206),
            red: Color32::from_rgb(186, 26, 26),              // #ba1a1a Error / Clip
            red_bottom: Color32::from_rgb(156, 20, 20),
            red_dim: Color32::from_rgb(242, 218, 216),
            focus: Color32::from_rgb(14, 116, 144),           // Signal Teal focus outline
            wave_left: Color32::from_rgb(196, 91, 24),        // #c45b18 Signal Amber
            wave_right: Color32::from_rgb(14, 116, 144),      // #0e7490 Signal Teal
            edit_highlight: Color32::from_rgb(217, 119, 6),   // #d97706 Warm Tungsten
            btn_top: Color32::from_rgb(244, 242, 238),        // #f4f2ee Bead-blasted silver top
            btn_bottom: Color32::from_rgb(222, 217, 209),     // #ded9d1 Bead-blasted silver bottom
            btn_hover_top: Color32::from_rgb(250, 248, 245),
            btn_hover_bottom: Color32::from_rgb(232, 228, 220),
            btn_pressed_top: Color32::from_rgb(212, 207, 199),
            btn_pressed_bottom: Color32::from_rgb(226, 222, 214),
            knob_top: Color32::from_rgb(244, 242, 238),       // Milled Aluminum Knob
            knob_bottom: Color32::from_rgb(212, 207, 199),
            knob_cap_top: Color32::from_rgb(250, 249, 246),
            knob_cap_bottom: Color32::from_rgb(228, 224, 217),
            knob_pointer: Color32::from_rgb(18, 20, 23),      // High-contrast black pointer notch
            vu_face_top: Color32::from_rgb(251, 248, 238),    // #fbf8ee Warm cream parchment
            vu_face_bottom: Color32::from_rgb(240, 234, 214),
            vu_scale: Color32::from_rgb(44, 42, 38),
            vu_needle: Color32::from_rgb(18, 20, 23),
        }
    }

    /// Exakte Tokens aus StitchMCP: "Obsidian Waveform" (Dark Mode)
    pub const fn obsidian() -> Self {
        Self {
            is_dark: true,
            bg_inset: Color32::from_rgb(13, 14, 17),          // #0d0e11 Recessed Wells
            bg_window: Color32::from_rgb(18, 19, 22),         // #121316 App Shell / Base Layer
            bg_toolbar: Color32::from_rgb(26, 28, 34),        // #1a1c22 Deck Surface
            bg_panel: Color32::from_rgb(27, 29, 35),          // #1b1d23 Deck Module Frame
            bg_card: Color32::from_rgb(35, 38, 47),           // #23262f Component Faceplate
            border_subtle: Color32::from_rgb(46, 50, 61),     // #2e323d Boundary Hairline
            border_mid: Color32::from_rgb(66, 72, 86),
            text_primary: Color32::from_rgb(227, 226, 230),   // #e3e2e6 on-surface
            text_secondary: Color32::from_rgb(188, 192, 204),
            text_muted: Color32::from_rgb(138, 145, 160),
            text_on_accent: Color32::from_rgb(42, 23, 0),     // #2a1700 on-primary-fixed
            accent_top: Color32::from_rgb(255, 193, 116),     // #ffc174 primary
            accent_bottom: Color32::from_rgb(245, 158, 11),   // #f59e0b primary-container
            accent_soft: Color32::from_rgb(52, 38, 18),
            teal: Color32::from_rgb(16, 185, 129),            // #10b981 Phosphor Green
            teal_dim: Color32::from_rgb(20, 46, 38),
            amber: Color32::from_rgb(245, 158, 11),           // #f59e0b Warm Amber
            amber_dim: Color32::from_rgb(54, 38, 16),
            red: Color32::from_rgb(239, 68, 68),              // #ef4444 Peak Clip
            red_bottom: Color32::from_rgb(198, 36, 42),
            red_dim: Color32::from_rgb(56, 22, 24),
            focus: Color32::from_rgb(255, 185, 95),           // #ffb95f surface-tint
            wave_left: Color32::from_rgb(245, 158, 11),       // #f59e0b Warm Amber
            wave_right: Color32::from_rgb(78, 222, 163),      // #4edea3 secondary
            edit_highlight: Color32::from_rgb(255, 193, 116),
            btn_top: Color32::from_rgb(48, 52, 62),
            btn_bottom: Color32::from_rgb(35, 38, 47),        // #23262f
            btn_hover_top: Color32::from_rgb(58, 63, 75),
            btn_hover_bottom: Color32::from_rgb(44, 48, 59),
            btn_pressed_top: Color32::from_rgb(30, 32, 40),
            btn_pressed_bottom: Color32::from_rgb(38, 42, 51),
            knob_top: Color32::from_rgb(84, 89, 102),
            knob_bottom: Color32::from_rgb(50, 54, 64),
            knob_cap_top: Color32::from_rgb(70, 75, 87),
            knob_cap_bottom: Color32::from_rgb(46, 50, 60),
            knob_pointer: Color32::from_rgb(245, 158, 11),
            vu_face_top: Color32::from_rgb(36, 28, 18),
            vu_face_bottom: Color32::from_rgb(16, 15, 14),
            vu_scale: Color32::from_rgb(168, 156, 138),
            vu_needle: Color32::from_rgb(255, 196, 104),
        }
    }
}

pub fn palette() -> Palette {
    match theme_mode() {
        ThemeMode::Silverface => Palette::silverface(),
        ThemeMode::Obsidian => Palette::obsidian(),
    }
}

// Kompatibilitäts-Konstanten für bestehende Module
#[allow(dead_code)]
pub const BG_INSET: Color32 = Color32::from_rgb(242, 244, 248);
#[allow(dead_code)]
pub const BG_WINDOW: Color32 = Color32::from_rgb(233, 236, 241);
#[allow(dead_code)]
pub const BG_CARD: Color32 = Color32::from_rgb(245, 247, 250);
#[allow(dead_code)]
pub const BORDER_LIGHT: Color32 = Color32::from_rgb(160, 168, 182);
#[allow(dead_code)]
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(76, 84, 98);
#[allow(dead_code)]
pub const TEXT_MUTED: Color32 = Color32::from_rgb(112, 120, 134);
#[allow(dead_code)]
pub const LED_GREEN: Color32 = Color32::from_rgb(14, 126, 150);
#[allow(dead_code)]
pub const LED_AMBER: Color32 = Color32::from_rgb(210, 108, 20);
#[allow(dead_code)]
pub const EDIT_HIGHLIGHT: Color32 = Color32::from_rgb(216, 114, 24);
#[allow(dead_code)]
pub const LED_RED: Color32 = Color32::from_rgb(214, 48, 44);
#[allow(dead_code)]
pub const WAVE_LEFT: Color32 = Color32::from_rgb(206, 104, 34);
#[allow(dead_code)]
pub const WAVE_RIGHT: Color32 = Color32::from_rgb(20, 124, 152);
#[allow(dead_code)]
pub const ACCENT: Color32 = Color32::from_rgb(216, 114, 24);
#[allow(dead_code)]
pub const RECORD_ACCENT: Color32 = LED_RED;
#[allow(dead_code)]
pub const CUT_ACCENT: Color32 = ACCENT;
#[allow(dead_code)]
pub const FX_ACCENT: Color32 = LED_GREEN;

// Einheitliche Geometrie-Konstanten
pub const HEIGHT_PRIMARY: f32 = 28.0;
pub const HEIGHT_CONTROL: f32 = 25.0;
pub const HEIGHT_COMPACT: f32 = 22.0;

pub const RADIUS_CONTROL: f32 = 7.0;
pub const RADIUS_WELL: f32 = 8.0;
pub const RADIUS_BAY: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BevelStyle {
    Neutral,
    Primary,
    Record,
    Engaged,
    Compact,
    CompactEngaged,
}

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

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
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
    let p = palette();
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

/// Flächig abgesetzter Funktionsbereich (Silverface-Card im Light Mode / Graphit-Bay im Dark Mode).
pub fn surface_bay<R>(ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette();
    let frame = Frame::none()
        .fill(p.bg_panel)
        .rounding(RADIUS_BAY)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0));
    frame
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui)
        })
        .inner
}

/// Klarer Bereichs-Header mit Haupttitel und kurzer Einzeiler-Erklärung.
pub fn bay_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = palette();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(title)
                .size(12.8)
                .strong()
                .color(p.text_primary),
        );
        if !subtitle.is_empty() {
            ui.label(RichText::new(subtitle).size(11.2).color(p.text_muted));
        }
    });
}

pub fn bevel_button(ui: &mut Ui, label: &str, style: BevelStyle) -> egui::Response {
    bevel_button_sized(ui, label, 0.0, style)
}

pub fn bevel_button_sized(
    ui: &mut Ui,
    label: &str,
    min_width: f32,
    style: BevelStyle,
) -> egui::Response {
    let p = palette();
    let height = match style {
        BevelStyle::Primary | BevelStyle::Record => HEIGHT_PRIMARY,
        BevelStyle::Compact | BevelStyle::CompactEngaged => HEIGHT_COMPACT,
        BevelStyle::Neutral | BevelStyle::Engaged => HEIGHT_CONTROL,
    };
    let is_compact = matches!(style, BevelStyle::Compact | BevelStyle::CompactEngaged);
    let radius = if is_compact { 5.5 } else { RADIUS_CONTROL };
    let font_size = if is_compact { 11.2 } else { 12.0 };
    let pad_x = if is_compact { 5.0 } else { 10.0 };

    let font = egui::FontId::proportional(font_size);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, p.text_primary);
    let width = (galley.size().x + pad_x * 2.0).max(min_width);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());

    let enabled = ui.is_enabled();
    let pressed = enabled && response.is_pointer_button_down_on();
    let hovered = enabled && response.hovered();

    let (top, bottom, text_col, stroke) = match style {
        BevelStyle::Primary | BevelStyle::Engaged | BevelStyle::CompactEngaged => {
            if !enabled {
                (p.bg_card, p.bg_card, p.text_muted, None)
            } else if pressed {
                (p.accent_bottom, p.accent_top, p.text_on_accent, None)
            } else if hovered {
                (
                    lerp_color(p.accent_top, Color32::WHITE, 0.12),
                    p.accent_top,
                    p.text_on_accent,
                    None,
                )
            } else {
                (p.accent_top, p.accent_bottom, p.text_on_accent, None)
            }
        }
        BevelStyle::Record => {
            if !enabled {
                (p.red_dim, p.red_dim, p.text_muted, None)
            } else if pressed {
                (p.red_bottom, p.red, Color32::WHITE, None)
            } else if hovered {
                (
                    lerp_color(p.red, Color32::WHITE, 0.10),
                    p.red,
                    Color32::WHITE,
                    None,
                )
            } else {
                (p.red, p.red_bottom, Color32::WHITE, None)
            }
        }
        BevelStyle::Neutral | BevelStyle::Compact => {
            if !enabled {
                (p.bg_card, p.bg_card, p.text_muted, Some(p.border_subtle))
            } else if pressed {
                (
                    p.btn_pressed_top,
                    p.btn_pressed_bottom,
                    p.text_primary,
                    Some(p.border_mid),
                )
            } else if hovered {
                (
                    p.btn_hover_top,
                    p.btn_hover_bottom,
                    p.text_primary,
                    Some(p.border_mid),
                )
            } else {
                (
                    p.btn_top,
                    p.btn_bottom,
                    p.text_primary,
                    Some(p.border_subtle),
                )
            }
        }
    };

    paint_bevel_surface(ui.painter(), rect, radius, top, bottom, pressed, stroke);

    let text_pos = Pos2::new(
        rect.center().x,
        rect.center().y + if pressed { 0.5 } else { 0.0 },
    );
    ui.painter().text(
        text_pos,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        text_col,
    );

    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    draw_keyboard_focus(ui, &response);
    response
}

/// Symmetrisches Pill-Segmented-Control (Silverface & Obsidian, aktives Segment in Kupfer-Amber).
pub fn segmented_control<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    current: &mut T,
    items: &[(T, &str, &str)],
    height: f32,
) -> bool {
    let p = palette();
    let mut changed = false;
    let enabled = ui.is_enabled();
    let pad = 2.5;
    let font = egui::FontId::proportional(if height >= 26.0 { 12.0 } else { 11.4 });
    let track_r = height * 0.5;
    let thumb_r = (height - pad * 2.0) * 0.5;

    let max_seg_w = items
        .iter()
        .map(|(_, label, _)| {
            let w = ui
                .painter()
                .layout_no_wrap((*label).to_string(), font.clone(), p.text_primary)
                .size()
                .x;
            (w + 20.0).max(if height >= 26.0 { 72.0 } else { 56.0 })
        })
        .fold(0.0_f32, f32::max);
    let total_width = max_seg_w * items.len() as f32 + pad * 2.0;

    ui.push_id(id_salt, |ui| {
        let (track_rect, _) =
            ui.allocate_exact_size(Vec2::new(total_width, height), egui::Sense::hover());
        ui.painter().rect_filled(track_rect, track_r, p.bg_inset);
        ui.painter()
            .rect_stroke(track_rect.shrink(0.5), track_r, Stroke::new(0.75, p.border_subtle));

        let mut x = track_rect.min.x + pad;
        for (idx, &(value, label, shortcut)) in items.iter().enumerate() {
            let seg_w = max_seg_w;
            let seg_rect = Rect::from_min_size(
                Pos2::new(x, track_rect.min.y + pad),
                Vec2::new(seg_w, height - pad * 2.0),
            );
            x += seg_w;

            let seg_id = ui.make_persistent_id(idx);
            let resp = ui.interact(seg_rect, seg_id, egui::Sense::click());
            let selected = *current == value;
            let pressed = enabled && resp.is_pointer_button_down_on();
            let hovered = enabled && resp.hovered();

            if selected {
                let (top, bot) = if pressed {
                    (p.accent_bottom, p.accent_top)
                } else if hovered {
                    (lerp_color(p.accent_top, Color32::WHITE, 0.1), p.accent_top)
                } else {
                    (p.accent_top, p.accent_bottom)
                };
                paint_bevel_surface(ui.painter(), seg_rect, thumb_r, top, bot, pressed, None);
            } else if hovered {
                let hover_fill = if p.is_dark {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, 12)
                };
                ui.painter().rect_filled(seg_rect, thumb_r, hover_fill);
            }

            let text_col = if !enabled {
                p.text_muted
            } else if selected {
                p.text_on_accent
            } else if hovered {
                p.text_primary
            } else {
                p.text_secondary
            };
            ui.painter().text(
                seg_rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                text_col,
            );

            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, label)
            });
            draw_keyboard_focus(ui, &resp);
            let resp = if shortcut.is_empty() {
                resp
            } else {
                resp.on_hover_text(shortcut)
            };
            if enabled && resp.clicked() && !selected {
                *current = value;
                changed = true;
            }
        }
    });
    changed
}

pub fn panel_title(ui: &mut Ui, title: &str) {
    let p = palette();
    ui.label(
        RichText::new(title)
            .size(12.2)
            .strong()
            .color(p.text_primary),
    );
}

pub fn section_title(ui: &mut Ui, title: &str, _accent: Color32) {
    panel_title(ui, title);
}

#[allow(dead_code)]
pub fn studio_wordmark(ui: &mut Ui) {
    let p = palette();
    let (mark, _) = ui.allocate_exact_size(Vec2::new(26.0, 20.0), egui::Sense::hover());
    let painter = ui.painter();
    paint_bevel_surface(
        painter,
        mark,
        5.0,
        p.accent_top,
        p.accent_bottom,
        false,
        None,
    );
    let left = mark.center() - Vec2::new(5.5, 0.0);
    let right = mark.center() + Vec2::new(5.5, 0.0);
    for reel in [left, right] {
        painter.circle_stroke(reel, 3.6, Stroke::new(1.2, p.text_on_accent));
        painter.circle_filled(reel, 1.2, p.text_on_accent);
    }
    ui.label(
        RichText::new("TapeDigizer")
            .size(13.5)
            .strong()
            .color(p.text_primary),
    );
}

pub fn studio_panel(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
    surface_bay(ui, |ui| {
        panel_title(ui, title);
        ui.add_space(6.0);
        contents(ui);
    });
}

pub fn primary_button(label: impl Into<egui::WidgetText>) -> Button<'static> {
    let p = palette();
    let label = label.into().strong().color(p.text_on_accent);
    Button::new(label)
        .fill(p.accent_top)
        .stroke(Stroke::NONE)
        .rounding(RADIUS_CONTROL)
        .min_size(Vec2::new(84.0, HEIGHT_PRIMARY))
}

pub fn mode_button(label: impl Into<egui::WidgetText>, selected: bool) -> Button<'static> {
    let p = palette();
    let label = label.into();
    let button = Button::new(if selected {
        label.strong().color(p.text_on_accent)
    } else {
        label.color(p.text_secondary)
    })
    .selected(selected)
    .rounding(RADIUS_CONTROL)
    .min_size(Vec2::new(0.0, HEIGHT_CONTROL));
    if selected {
        button.fill(p.accent_top).stroke(Stroke::NONE)
    } else {
        button
            .fill(p.btn_bottom)
            .stroke(Stroke::new(0.75, p.border_subtle))
    }
}

pub fn time_display(ui: &mut Ui, value: String) {
    let p = palette();
    let desired = Vec2::new(
        ui.painter()
            .layout_no_wrap(value.clone(), egui::FontId::monospace(13.5), p.amber)
            .size()
            .x
            + 20.0,
        HEIGHT_PRIMARY,
    );
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    draw_recessed_well(ui.painter(), rect, RADIUS_WELL, p.bg_inset);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        value,
        egui::FontId::monospace(13.5),
        p.amber,
    );
}

#[allow(dead_code)]
pub fn studio_style() -> egui::Style {
    studio_style_for(theme_mode())
}

pub fn studio_style_for(mode: ThemeMode) -> egui::Style {
    set_theme_mode(mode);
    let mut style = egui::Style::default();
    style.visuals = studio_visuals(palette());
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

/// Vertiefte Display-Fläche (weich abgerundet, theme-aware).
pub fn draw_recessed_well(painter: &egui::Painter, rect: Rect, rounding: f32, fill: Color32) {
    let p = palette();
    painter.rect_filled(rect, rounding, fill);
    painter.rect_stroke(
        rect.shrink(0.5),
        rounding,
        Stroke::new(0.75, p.border_subtle),
    );
}

pub fn meter_readout(ui: &mut Ui, label: &str, value: String, color: Color32) {
    let p = palette();
    ui.add(
        egui::Label::new(
            RichText::new(label)
                .size(11.0)
                .monospace()
                .color(p.text_muted),
        )
        .truncate(),
    );
    ui.add(egui::Label::new(RichText::new(value).size(19.0).monospace().color(color)).truncate());
}

pub fn draw_keyboard_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        let p = palette();
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            RADIUS_CONTROL + 1.0,
            Stroke::new(2.0, p.focus),
        );
    }
}

pub fn transport_button(label: &str, engaged: bool) -> Button<'_> {
    let p = palette();
    Button::new(RichText::new(label).strong().color(if engaged {
        p.text_on_accent
    } else {
        p.text_primary
    }))
    .fill(if engaged { p.accent_top } else { p.btn_bottom })
    .stroke(if engaged {
        Stroke::NONE
    } else {
        Stroke::new(0.75, p.border_subtle)
    })
    .rounding(RADIUS_CONTROL)
    .min_size(Vec2::new(64.0, HEIGHT_PRIMARY))
}

pub fn rack_columns(ui: &mut Ui, count: usize, contents: impl FnOnce(&mut [Ui])) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.columns(count, |columns| {
            for column in columns.iter_mut() {
                column.spacing_mut().item_spacing.x = 6.0;
            }
            contents(columns);
        });
    });
}

/// Nummerierte Stitch-Modul-Card (z. B. "01  Tiefen (Low Shelf) · Analog Baxandall").
pub fn module_card<R>(
    ui: &mut Ui,
    badge: &str,
    title: &str,
    subtitle: &str,
    accent: Color32,
    contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = palette();
    let card_bg = if p.is_dark { p.bg_card } else { Color32::WHITE };
    Frame::none()
        .fill(card_bg)
        .stroke(Stroke::new(0.75, p.border_subtle))
        .rounding(9.0)
        .inner_margin(egui::Margin::symmetric(9.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Kleiner nummerierter Badge ("01")
                let badge_bg = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), if p.is_dark { 42 } else { 26 });
                Frame::none()
                    .fill(badge_bg)
                    .rounding(4.0)
                    .inner_margin(egui::Margin::symmetric(4.5, 1.5))
                    .show(ui, |ui| {
                        ui.label(RichText::new(badge).monospace().size(9.5).strong().color(accent));
                    });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(title).size(11.5).strong().color(p.text_primary));
                    if !subtitle.is_empty() {
                        ui.label(RichText::new(subtitle).size(9.8).color(p.text_muted));
                    }
                });
            });
            ui.add_space(3.0);
            contents(ui)
        })
        .inner
}

/// Kompaktes dunkles vertikales Stereo-LED-Meter (wie im Stitch Mix-/Ausgangs-Panel).
pub fn draw_vertical_stereo_meter(
    ui: &mut Ui,
    peak_l_db: f32,
    peak_r_db: f32,
    is_clipping: bool,
    height: f32,
) -> bool {
    let p = palette();
    let desired = Vec2::new(66.0, height.max(96.0));
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    let painter = ui.painter();

    // Immer dunkles OLED-Gehäuse wie im Stitch-Entwurf
    let oled_bg = Color32::from_rgb(15, 17, 22);
    painter.rect_filled(rect, 8.0, oled_bg);
    painter.rect_stroke(rect.shrink(0.5), 8.0, Stroke::new(0.75, p.border_subtle));

    let peak_max = peak_l_db.max(peak_r_db);
    let peak_txt = if peak_max <= -59.5 {
        "-∞ dB".to_string()
    } else {
        format!("{peak_max:+.1}")
    };
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 8.0),
        egui::Align2::CENTER_TOP,
        peak_txt,
        egui::FontId::monospace(9.5),
        if is_clipping {
            Color32::from_rgb(239, 68, 68)
        } else {
            Color32::from_rgb(78, 222, 163)
        },
    );

    let bar_top = rect.min.y + 24.0;
    let bar_bot = rect.max.y - 18.0;
    let bar_h = (bar_bot - bar_top).max(20.0);
    let segments = 14usize;
    let gap = 2.0;
    let seg_h = ((bar_h - (segments as f32 - 1.0) * gap) / segments as f32).max(2.0);

    for (col_idx, db) in [(0usize, peak_l_db), (1usize, peak_r_db)] {
        let x = rect.center().x + if col_idx == 0 { -13.0 } else { 3.0 };
        let norm = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let lit = (norm * segments as f32).round() as usize;
        for i in 0..segments {
            let y = bar_bot - (i as f32 + 1.0) * seg_h - (i as f32) * gap;
            let seg_db = -60.0 + ((i + 1) as f32 / segments as f32) * 60.0;
            let (on_col, off_col) = if seg_db >= -1.0 {
                (Color32::from_rgb(239, 68, 68), Color32::from_rgb(52, 22, 24))
            } else if seg_db >= -6.0 {
                (Color32::from_rgb(245, 158, 11), Color32::from_rgb(52, 38, 18))
            } else {
                (Color32::from_rgb(16, 185, 129), Color32::from_rgb(18, 44, 36))
            };
            let sr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(10.0, seg_h));
            painter.rect_filled(sr, 1.5, if i < lit { on_col } else { off_col });
        }
        painter.text(
            Pos2::new(x + 5.0, rect.max.y - 5.0),
            egui::Align2::CENTER_BOTTOM,
            if col_idx == 0 { "L" } else { "R" },
            egui::FontId::monospace(9.0),
            Color32::from_rgb(156, 163, 175),
        );
    }

    response.clicked()
}

/// Silber-Aluminium Drehregler (glatt, leichter Verlauf, glatter 270° Farbbogen + AccessKit).
#[allow(dead_code)]
pub fn rotary(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) {
    rotary_colored_sized(ui, label, value, range, suffix, palette().accent_top, 46.0);
}

pub fn rotary_colored(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    arc_color: Color32,
) {
    rotary_colored_sized(ui, label, value, range, suffix, arc_color, 46.0);
}

pub fn rotary_colored_sized(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    arc_color: Color32,
    knob_size: f32,
) {
    let p = palette();
    let k = knob_size.clamp(34.0, 60.0);
    let scale = k / 56.0;
    ui.push_id(label, |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let name = ui.label(RichText::new(label).size(10.8).color(p.text_secondary));
            let (min, max) = (*range.start(), *range.end());
            let span = (max - min).max(1e-6);
            let use_log = min > 0.0 && (max / min) >= 20.0;
            let to_norm = |v: f32| -> f32 {
                if use_log {
                    ((v.clamp(min, max) / min).ln() / (max / min).ln()).clamp(0.0, 1.0)
                } else {
                    ((v - min) / span).clamp(0.0, 1.0)
                }
            };
            let from_norm = |u: f32| -> f32 {
                let u = u.clamp(0.0, 1.0);
                if use_log {
                    (min * (max / min).powf(u)).clamp(min, max)
                } else {
                    (min + u * span).clamp(min, max)
                }
            };
            let bipolar = min < 0.0 && max > 0.0;

            let mut response =
                ui.allocate_response(Vec2::splat(k), egui::Sense::click_and_drag());
            let shift = ui.input(|input| input.modifiers.shift);
            let drag_norm_per_px = if shift { 1.0 / 900.0 } else { 1.0 / 220.0 };
            let key_step_norm = if shift { 0.0012 } else { 0.0065 };
            let drag_anchor_id = response.id.with("rotary_drag_anchor");
            let mut next_value: Option<f32> = None;

            if ui.is_enabled() {
                if response.double_clicked() && bipolar {
                    next_value = Some(0.0_f32.clamp(min, max));
                }

                let pointer_pos = response
                    .interact_pointer_pos()
                    .or_else(|| ui.ctx().pointer_latest_pos());
                let active_drag = response.dragged() || response.is_pointer_button_down_on();
                if active_drag && ui.data(|d| d.get_temp::<(Pos2, f32)>(drag_anchor_id)).is_none() {
                    let origin = ui
                        .input(|i| i.pointer.press_origin())
                        .or(pointer_pos);
                    if let Some(pos) = origin {
                        ui.data_mut(|d| d.insert_temp(drag_anchor_id, (pos, to_norm(*value))));
                    }
                }
                if active_drag {
                    if let Some(pos) = pointer_pos {
                        if let Some((start_pos, start_norm)) =
                            ui.data(|d| d.get_temp::<(Pos2, f32)>(drag_anchor_id))
                        {
                            let dy = start_pos.y - pos.y;
                            if dy.abs() > 0.25 {
                                next_value = Some(from_norm(start_norm + dy * drag_norm_per_px));
                            }
                        }
                    }
                } else {
                    ui.data_mut(|d| d.remove::<(Pos2, f32)>(drag_anchor_id));
                }

                // Mausrad / Trackpad-Geste direkt über dem Poti
                if response.hovered() && !active_drag {
                    let scroll_y = ui.input(|input| input.smooth_scroll_delta.y);
                    if scroll_y.abs() > 0.1 {
                        let scroll_sens = if shift { 0.0005 } else { 0.0025 };
                        next_value = Some(from_norm(to_norm(*value) + scroll_y * scroll_sens));
                    }
                }
            }

            let mut keys = if response.has_focus() {
                ui.input(|input| {
                    i32::from(
                        input.key_pressed(egui::Key::ArrowUp)
                            || input.key_pressed(egui::Key::ArrowRight),
                    ) - i32::from(
                        input.key_pressed(egui::Key::ArrowDown)
                            || input.key_pressed(egui::Key::ArrowLeft),
                    )
                })
            } else {
                0
            };
            use egui::accesskit::{Action, ActionData};
            let requested_value = ui.input(|input| {
                keys += input.num_accesskit_action_requests(response.id, Action::Increment) as i32;
                keys -= input.num_accesskit_action_requests(response.id, Action::Decrement) as i32;
                input
                    .accesskit_action_requests(response.id, Action::SetValue)
                    .filter_map(|request| match request.data {
                        Some(ActionData::NumericValue(value)) => Some(value as f32),
                        Some(ActionData::Value(ref value)) => value.trim().parse().ok(),
                        _ => None,
                    })
                    .filter(|value: &f32| value.is_finite())
                    .last()
            });
            if ui.is_enabled()
                && (next_value.is_some() || keys != 0 || requested_value.is_some())
            {
                let next = requested_value
                    .or(next_value)
                    .unwrap_or_else(|| from_norm(to_norm(*value) + keys as f32 * key_step_norm));
                *value = next.clamp(min, max);
                response.mark_changed();
            }
            response.widget_info(|| {
                egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*value), label)
            });
            ui.ctx().accesskit_node_builder(response.id, |builder| {
                builder.set_min_numeric_value(f64::from(min));
                builder.set_max_numeric_value(f64::from(max));
                if ui.is_enabled() {
                    builder.add_action(Action::Increment);
                    builder.add_action(Action::Decrement);
                    builder.add_action(Action::SetValue);
                }
            });

            let painter = ui.painter();
            let center = response.rect.center();
            let fraction = to_norm(*value);
            let at = |unit: f32, radius: f32| {
                let angle = (unit - 0.5) * std::f32::consts::PI * 1.5;
                center + Vec2::new(angle.sin(), -angle.cos()) * radius
            };

            let zero_frac = if bipolar {
                to_norm(0.0)
            } else {
                0.0
            };

            // Glatter 270° Hintergrund-Bogen (wie in den Stitch-Panels)
            let arc_radius = 23.5 * scale;
            let bg_arc: Vec<Pos2> = (0..=32)
                .map(|i| at(i as f32 / 32.0, arc_radius))
                .collect();
            painter.add(Shape::line(bg_arc, Stroke::new((3.0 * scale).max(2.0), p.border_subtle)));

            // Glatter farbiger Wert-Bogen (von 0 bis fraction, oder bipolar von Mitte bis fraction)
            let (arc_start, arc_end) = if bipolar {
                (zero_frac.min(fraction), zero_frac.max(fraction))
            } else {
                (0.0, fraction)
            };
            if arc_end - arc_start > 0.005 {
                let segs = (((arc_end - arc_start) * 36.0).ceil() as usize).max(2);
                let val_arc: Vec<Pos2> = (0..=segs)
                    .map(|i| {
                        let u = arc_start + (arc_end - arc_start) * (i as f32 / segs as f32);
                        at(u, arc_radius)
                    })
                    .collect();
                painter.add(Shape::line(val_arc, Stroke::new((3.2 * scale).max(2.2), arc_color)));
            }

            // Weicher Schatten & glatte Silber-Aluminium-Kappe
            painter.circle_filled(
                center + Vec2::new(0.0, 1.2 * scale),
                17.0 * scale,
                Color32::from_rgba_unmultiplied(0, 0, 0, if p.is_dark { 115 } else { 30 }),
            );
            let knob_diam = 33.0 * scale;
            let knob_rect = Rect::from_center_size(center, Vec2::splat(knob_diam));
            let active_ring = if response.dragged() {
                Some(Color32::from_rgba_unmultiplied(arc_color.r(), arc_color.g(), arc_color.b(), 210))
            } else if response.hovered() {
                Some(Color32::from_rgba_unmultiplied(arc_color.r(), arc_color.g(), arc_color.b(), 120))
            } else {
                Some(p.border_mid)
            };
            paint_bevel_surface(
                painter,
                knob_rect,
                knob_diam * 0.5,
                p.knob_top,
                p.knob_bottom,
                response.dragged(),
                active_ring,
            );

            let cap_diam = 23.0 * scale;
            let cap_rect = Rect::from_center_size(center, Vec2::splat(cap_diam));
            paint_rounded_vertical_gradient(
                painter,
                cap_rect,
                cap_diam * 0.5,
                p.knob_cap_top,
                p.knob_cap_bottom,
            );

            // Klarer farbiger Zeigerstrich
            painter.line_segment(
                [at(fraction, 5.5 * scale), at(fraction, 14.2 * scale)],
                Stroke::new((2.4 * scale).max(1.8), arc_color),
            );
            if response.has_focus() {
                painter.circle_stroke(center, 27.5 * scale, Stroke::new(2.0, p.focus));
            }
            let cursor = if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            };
            let tip = if bipolar {
                "Hoch/Runter ziehen oder Mausrad · Shift = Fein · Doppelklick = 0"
            } else {
                "Hoch/Runter ziehen oder Mausrad · Shift = Fein"
            };
            response.on_hover_cursor(cursor).on_hover_text(tip);
            ui.add_sized(
                [66.0, 20.0],
                egui::DragValue::new(value)
                    .range(range)
                    .speed(span * 0.001)
                    .max_decimals(2)
                    .suffix(suffix),
            )
            .labelled_by(name.id)
            .on_hover_text(label);
        });
    });
}

/// Glatter Silber-Hardware-Fader (0..100 % für Monomix).
pub fn hardware_fader(ui: &mut Ui, label: &str, value: &mut f32) {
    hardware_fader_custom(ui, label, value, 0.0..=1.0, 88.0, true);
}

#[allow(dead_code)]
pub fn hardware_db_fader(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    height: f32,
) {
    hardware_fader_custom(ui, label, value, range, height, false);
}

fn hardware_fader_custom(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    track_h: f32,
    percent: bool,
) {
    let p = palette();
    let (min, max) = (*range.start(), *range.end());
    let span = (max - min).max(1e-5);
    ui.push_id(label, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(72.0, 0.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let name = ui.label(RichText::new(label).size(11.5).color(p.text_secondary));
                let (rect, mut response) =
                    ui.allocate_exact_size(Vec2::new(50.0, track_h), egui::Sense::click_and_drag());
                let enabled = ui.is_enabled();

                let cap_h = 18.0;
                let travel_top = rect.min.y + cap_h * 0.5 + 2.0;
                let travel_bot = rect.max.y - cap_h * 0.5 - 2.0;
                let travel_span = (travel_bot - travel_top).max(1.0);

                if enabled && (response.dragged() || response.clicked()) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let frac = ((travel_bot - pos.y) / travel_span).clamp(0.0, 1.0);
                        *value = (min + frac * span).clamp(min, max);
                        response.mark_changed();
                    }
                }
                if enabled && response.has_focus() {
                    let keys = ui.input(|input| {
                        i32::from(
                            input.key_pressed(egui::Key::ArrowUp)
                                || input.key_pressed(egui::Key::ArrowRight),
                        ) - i32::from(
                            input.key_pressed(egui::Key::ArrowDown)
                                || input.key_pressed(egui::Key::ArrowLeft),
                        )
                    });
                    if keys != 0 {
                        *value = (*value + keys as f32 * span * 0.02).clamp(min, max);
                        response.mark_changed();
                    }
                }

                response.widget_info(|| {
                    egui::WidgetInfo::slider(
                        enabled,
                        f64::from(if percent { *value * 100.0 } else { *value }),
                        label,
                    )
                });

                let painter = ui.painter();
                let cx = rect.center().x;

                for i in 0..=6 {
                    let frac = i as f32 / 6.0;
                    let y = travel_bot - frac * travel_span;
                    let is_zero = !percent && (min + frac * span).abs() < 0.5;
                    let len = if is_zero || (percent && i == 3) { 7.0 } else { 4.0 };
                    let col = if is_zero { p.accent_top } else { p.border_mid };
                    painter.line_segment(
                        [Pos2::new(cx - 11.0 - len, y), Pos2::new(cx - 11.0, y)],
                        Stroke::new(1.0, col),
                    );
                    painter.line_segment(
                        [Pos2::new(cx + 11.0, y), Pos2::new(cx + 11.0 + len, y)],
                        Stroke::new(1.0, col),
                    );
                }

                let slot_rect = Rect::from_min_max(
                    Pos2::new(cx - 3.0, travel_top - 4.0),
                    Pos2::new(cx + 3.0, travel_bot + 4.0),
                );
                draw_recessed_well(painter, slot_rect, 3.0, p.bg_inset);

                let norm = ((*value - min) / span).clamp(0.0, 1.0);
                let knob_y = travel_bot - norm * travel_span;
                let fill_rect = Rect::from_min_max(
                    Pos2::new(cx - 2.0, knob_y),
                    Pos2::new(cx + 2.0, travel_bot + 2.0),
                );
                painter.rect_filled(fill_rect, 2.0, p.accent_top);

                let cap_rect =
                    Rect::from_center_size(Pos2::new(cx, knob_y), Vec2::new(28.0, cap_h));
                paint_bevel_surface(
                    painter,
                    cap_rect,
                    5.0,
                    p.knob_top,
                    p.knob_bottom,
                    false,
                    Some(p.border_mid),
                );
                painter.line_segment(
                    [
                        Pos2::new(cap_rect.min.x + 5.0, knob_y),
                        Pos2::new(cap_rect.max.x - 5.0, knob_y),
                    ],
                    Stroke::new(1.8, p.accent_top),
                );

                draw_keyboard_focus(ui, &response);
                response.on_hover_cursor(egui::CursorIcon::ResizeVertical);

                if percent {
                    ui.add_sized(
                        [68.0, 22.0],
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(0.005)
                            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
                            .custom_parser(|text| {
                                text.trim()
                                    .trim_end_matches('%')
                                    .trim()
                                    .parse::<f64>()
                                    .ok()
                                    .map(|v| v / 100.0)
                            }),
                    )
                    .labelled_by(name.id)
                    .on_hover_text(label);
                } else {
                    ui.add_sized(
                        [68.0, 22.0],
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(0.1)
                            .max_decimals(1)
                            .suffix(" dB"),
                    )
                    .labelled_by(name.id)
                    .on_hover_text(label);
                }
            },
        );
    });
}

/// Animiertes Tape-Transport-Deck (Silverface Light & Obsidian Dark).
pub fn draw_tape_transport_deck(
    ui: &mut Ui,
    elapsed_secs: f64,
    nominal_total_secs: f64,
    is_running: bool,
    is_recording: bool,
    medium_label: &str,
) {
    let p = palette();
    let deck_h = ui.available_height().clamp(70.0, 148.0);
    let desired = Vec2::new(ui.available_width().clamp(210.0, 290.0), deck_h);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();

    draw_recessed_well(painter, rect, RADIUS_WELL, p.bg_inset);

    let progress = if nominal_total_secs > 1.0 {
        (elapsed_secs / nominal_total_secs).clamp(0.0, 1.0) as f32
    } else {
        ((elapsed_secs % 1800.0) / 1800.0) as f32
    };

    let scale = (deck_h / 118.0).clamp(0.7, 1.15);
    let reel_y = rect.min.y + 38.0 * scale;
    let left_c = Pos2::new(rect.min.x + rect.width() * 0.27, reel_y);
    let right_c = Pos2::new(rect.min.x + rect.width() * 0.73, reel_y);
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::{Action, ActionData, ActionRequest, Role};

    #[test]
    fn rotary_accepts_accessibility_values_and_keyboard_steps() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut value = 0.0;
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rotary(ui, "Song gain", &mut value, -24.0..=12.0, " dB");
            });
        };
        let output = ctx.run(egui::RawInput::default(), &mut draw);
        let tree = output.platform_output.accesskit_update.unwrap();
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Slider && node.name() == Some("Song gain"))
            .expect("the custom rotary exposes a named slider");
        let target = *target;
        let output = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    target,
                    action: Action::SetValue,
                    data: Some(ActionData::Value("3.5".into())),
                })],
                ..Default::default()
            },
            &mut draw,
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| *id == target)
                .unwrap()
                .1
                .numeric_value(),
            Some(3.5)
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::AccessKitActionRequest(ActionRequest {
                        target,
                        action: Action::Focus,
                        data: None,
                    }),
                    egui::Event::Key {
                        key: egui::Key::ArrowRight,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            &mut draw,
        );
        assert!(value > 3.5 && value < 4.0);
    }

    #[test]
    fn rotary_responds_to_vertical_pointer_drag() {
        let ctx = egui::Context::default();
        let screen = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 300.0)));
        let mut value = 0.0_f32;
        let knob_center = std::cell::Cell::new(Pos2::ZERO);
        let mut draw = |ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(Vec2::new(90.0, 110.0), |ui| {
                    rotary(ui, "Test gain", &mut value, -24.0..=12.0, " dB");
                    knob_center.set(ui.min_rect().center());
                });
            });
        };
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.0),
                ..Default::default()
            },
            &mut draw,
        );
        let center = knob_center.get();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.05),
                events: vec![egui::Event::PointerMoved(center)],
                ..Default::default()
            },
            &mut draw,
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.1),
                events: vec![egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            &mut draw,
        );
        let dragged_pos = center - Vec2::new(0.0, 40.0);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: screen,
                time: Some(0.2),
                events: vec![egui::Event::PointerMoved(dragged_pos)],
                ..Default::default()
            },
            &mut draw,
        );
        assert!(
            value > 2.0,
            "Dragging upward on rotary should increase value, got {value}"
        );
    }
}

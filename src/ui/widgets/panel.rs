use egui::{Color32, Frame, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, UiBuilder, Vec2};

use crate::ui::theme::{palette, HEIGHT_PRIMARY, RADIUS_BAY, RADIUS_WELL};

use super::paint::{draw_recessed_well, paint_bevel_surface};

#[derive(Clone, Copy)]
pub enum PanelStyle {
    Surface,
    Module,
    Inset,
}

impl PanelStyle {
    fn frame(self, ui: &Ui) -> Frame {
        let p = palette(ui.ctx());
        match self {
            Self::Surface => Frame::none()
                .fill(p.bg_panel)
                .rounding(RADIUS_BAY)
                .inner_margin(egui::Margin::symmetric(12.0, 10.0)),
            Self::Module => Frame::none()
                .fill(if p.is_dark { p.bg_card } else { Color32::WHITE })
                .stroke(Stroke::new(0.75, p.border_subtle))
                .rounding(9.0)
                .inner_margin(egui::Margin::symmetric(9.0, 6.0)),
            Self::Inset => Frame::none()
                .fill(p.bg_card)
                .rounding(RADIUS_WELL)
                .inner_margin(egui::Margin::symmetric(12.0, 10.0)),
        }
    }
}

pub fn surface_bay<R>(ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> R {
    PanelStyle::Surface
        .frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui)
        })
        .inner
}

/// Lay out panel contents once. Paint every panel at its column width and the
/// row's tallest content height, even when the contents change this frame.
pub fn panel_row(ui: &mut Ui, count: usize, style: PanelStyle, contents: impl FnOnce(&mut [Ui])) {
    let frame = style.frame(ui);
    rack_columns(ui, count, |columns| {
        let mut frames = Vec::with_capacity(count);
        let mut panels: Vec<_> = columns
            .iter_mut()
            .map(|column| {
                let bounds = column.available_rect_before_wrap();
                let background = column.painter().add(Shape::Noop);
                frames.push((bounds, background));
                let inner = bounds - frame.inner_margin;
                let mut panel = column.new_child(UiBuilder::new().max_rect(inner));
                let horizontal_clip =
                    Rect::from_x_y_ranges(inner.x_range(), column.clip_rect().y_range());
                panel.set_clip_rect(column.clip_rect().intersect(horizontal_clip));
                panel
            })
            .collect();
        contents(&mut panels);

        let bottom = panels
            .iter()
            .map(|panel| panel.min_rect().bottom() + frame.inner_margin.bottom)
            .fold(f32::NEG_INFINITY, f32::max);
        for (column, (bounds, background)) in columns.iter_mut().zip(frames) {
            let rect = Rect::from_min_max(bounds.min, Pos2::new(bounds.right(), bottom));
            column.painter().set(background, frame.paint(rect));
            column.allocate_rect(rect, Sense::hover());
        }
    });
}

/// Klarer Bereichs-Header mit Haupttitel und kurzer Einzeiler-Erklärung.
pub fn bay_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = palette(ui.ctx());
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

pub fn panel_title(ui: &mut Ui, title: &str) {
    let p = palette(ui.ctx());
    ui.label(
        RichText::new(title)
            .size(12.2)
            .strong()
            .color(p.text_primary),
    );
}

#[allow(dead_code)]
pub fn studio_wordmark(ui: &mut Ui) {
    let p = palette(ui.ctx());
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

pub fn time_display(ui: &mut Ui, value: String) {
    let p = palette(ui.ctx());
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

/// Kopf und Inhalt eines Effekt-Panels innerhalb einer Panelzeile.
pub fn module_contents<R>(
    ui: &mut Ui,
    badge: &str,
    title: &str,
    subtitle: &str,
    accent: Color32,
    contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = palette(ui.ctx());
    ui.horizontal(|ui| {
        // Kleiner nummerierter Badge ("01")
        let badge_bg = Color32::from_rgba_unmultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            if p.is_dark { 42 } else { 26 },
        );
        Frame::none()
            .fill(badge_bg)
            .rounding(4.0)
            .inner_margin(egui::Margin::symmetric(4.5, 1.5))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(badge)
                        .monospace()
                        .size(9.5)
                        .strong()
                        .color(accent),
                );
            });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(title)
                    .size(11.5)
                    .strong()
                    .color(p.text_primary),
            );
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(9.8).color(p.text_muted));
            }
        });
    });
    ui.add_space(3.0);
    contents(ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn panel_row_uses_equal_edges_and_follows_the_tallest_content() {
        let ctx = Context::default();
        let mut heights = Vec::new();
        for content_height in [85.0, 37.0] {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0))),
                ..Default::default()
            };
            let mut calls = 0;
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    panel_row(ui, 2, PanelStyle::Surface, |panels| {
                        for (panel, height) in panels.iter_mut().zip([18.0, content_height]) {
                            calls += 1;
                            panel.add_space(height);
                        }
                    });
                });
            });
            assert_eq!(calls, 2, "Interactive panel content runs once per frame");
            let frames: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(shape) if shape.fill == palette(&ctx).bg_panel => Some(shape.rect),
                    _ => None,
                })
                .collect();
            let [left, right] = frames.as_slice() else {
                panic!("Expected two panel backgrounds, got {frames:?}");
            };
            assert_eq!(left.top(), right.top());
            assert_eq!(left.bottom(), right.bottom());
            assert_eq!(left.width(), right.width());
            assert_eq!(right.left() - left.right(), 12.0);
            heights.push(left.height());
        }
        assert!(
            heights[0] > heights[1],
            "The row shrinks when its content shrinks"
        );
    }
}

use egui::{Align, ComboBox, InnerResponse, Layout, TextWrapMode, Ui, Vec2};

use crate::ui::theme::HEIGHT_CONTROL;

/// Constrain the selected label and menu to the width allocated by the view.
pub fn bounded_combo<R>(
    ui: &mut Ui,
    combo: ComboBox,
    width: f32,
    choices: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    ui.allocate_ui_with_layout(
        Vec2::new(width, HEIGHT_CONTROL),
        Layout::left_to_right(Align::Center),
        |ui| {
            combo.width(width).truncate().show_ui(ui, |ui| {
                ui.set_max_width(width);
                ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                choices(ui)
            })
        },
    )
    .inner
}

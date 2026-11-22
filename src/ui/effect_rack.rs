use egui::{RichText, Sense, Ui, Vec2};

use super::theme::palette;
use super::widgets::button::{bevel_button_sized, BevelStyle};
use super::widgets::paint::draw_recessed_well;
use crate::audio::meter::linear_to_db;
use crate::audio::processing::{EffectStage, MonitorStage, SongSettings};
use crate::audio::stage_meter::StageLevels;
use crate::i18n::Language;

fn name(stage: EffectStage, lang: Language) -> &'static str {
    match stage {
        EffectStage::Channels if lang == Language::De => "Kanäle",
        EffectStage::Channels => "Channels",
        EffectStage::Restoration if lang == Language::De => "Restauration",
        EffectStage::Restoration => "Restoration",
        EffectStage::Equalizer => "Equalizer",
        EffectStage::Expander => "Expander",
        EffectStage::Compressor => "Compressor",
        EffectStage::Limiter => "Limiter",
    }
}

pub(super) fn selector(
    ui: &mut Ui,
    selected: &mut MonitorStage,
    settings: &SongSettings,
    lang: Language,
) {
    let t = |de, en| if lang == Language::De { de } else { en };
    for slot in settings.effect_chain.slots() {
        let selected_stage = MonitorStage::Insert(slot.stage);
        let active = settings.effect_active(slot.stage) && !settings.bypass;
        let indicator = if active { "●" } else { "○" };
        let label = format!("{indicator} {}", name(slot.stage, lang));
        let style = if *selected == selected_stage {
            BevelStyle::CompactEngaged
        } else {
            BevelStyle::Compact
        };
        if bevel_button_sized(ui, &label, 91.0, style)
            .on_hover_text(if active {
                t("Aktiv", "Active")
            } else {
                t("Umgangen", "Bypassed")
            })
            .clicked()
        {
            *selected = selected_stage;
        }
    }
    let style = if *selected == MonitorStage::Mix {
        BevelStyle::CompactEngaged
    } else {
        BevelStyle::Compact
    };
    if bevel_button_sized(ui, t("Hauptmix", "Main mix"), 91.0, style)
        .on_hover_text(t("Fest am Kettenende", "Fixed at the end of the chain"))
        .clicked()
    {
        *selected = MonitorStage::Mix;
    }
}

pub(super) fn commands(
    ui: &mut Ui,
    selected: MonitorStage,
    settings: &mut SongSettings,
    channel: usize,
    lang: Language,
) {
    let MonitorStage::Insert(stage) = selected else {
        return;
    };
    let t = |de, en| if lang == Language::De { de } else { en };
    let mut active = settings.effect_active(stage);
    if ui
        .checkbox(&mut active, t("Effekt aktiv", "Insert active"))
        .changed()
    {
        settings.set_effect_active(stage, active, channel);
    }
    let index = settings.effect_chain.position(stage);
    let last = settings.effect_chain.slots().len() - 1;
    for (offset, enabled, label, tip) in [
        (
            -1,
            index > 0,
            "←",
            t("Eine Position nach links", "Move one slot left"),
        ),
        (
            1,
            index < last,
            "→",
            t("Eine Position nach rechts", "Move one slot right"),
        ),
    ] {
        let clicked = ui
            .add_enabled_ui(enabled, |ui| {
                bevel_button_sized(ui, label, 26.0, BevelStyle::Compact)
                    .on_hover_text(tip)
                    .clicked()
            })
            .inner;
        if clicked {
            settings.effect_chain.move_by(stage, offset);
        }
    }
}

pub(super) fn comparison(ui: &mut Ui, levels: Option<StageLevels>, lang: Language) {
    let t = |de, en| if lang == Language::De { de } else { en };
    let colors = palette(ui.ctx());
    let level = |x: f32| {
        if x > 0.0 {
            linear_to_db(x).max(-96.0)
        } else {
            f32::NEG_INFINITY
        }
    };
    let label = |gain: Option<f32>| match gain {
        Some(gain) if gain.is_finite() => format!("{gain:>5.1}"),
        Some(_) => "   −∞".into(),
        None => "    —".into(),
    };

    for (name, detail, peak, color) in [
        (
            t("Vor", "Before"),
            t("Spitzenpegel am Eingang der Stufe", "Stage input peak"),
            levels.map(|x| x.before),
            colors.text_secondary,
        ),
        (
            t("Nach", "After"),
            t("Spitzenpegel am Ausgang der Stufe", "Stage output peak"),
            levels.map(|x| x.after),
            colors.accent_top,
        ),
        (
            "Δ",
            t(
                "Spitzenpegel des Differenzsignals (Ausgang minus Eingang)",
                "Difference signal peak (output minus input)",
            ),
            levels.map(|x| x.difference),
            colors.teal,
        ),
    ] {
        ui.label(RichText::new(name).small()).on_hover_text(detail);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(31.0, 8.0), Sense::hover());
        draw_recessed_well(ui.painter(), rect, 2.0, colors.bg_inset);
        if let Some(peak) = peak {
            let fraction = ((level(peak) + 60.0) / 66.0).clamp(0.0, 1.0);
            let fill = egui::Rect::from_min_size(
                rect.min,
                Vec2::new(rect.width() * fraction, rect.height()),
            );
            ui.painter().rect_filled(fill, 2.0, color);
        }
        ui.label(RichText::new(label(peak.map(level))).monospace().small());
    }

    ui.label(RichText::new("dBFS").small());
}

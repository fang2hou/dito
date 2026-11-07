//! The language selector above the capsule.

use std::rc::Rc;

use dito_core::config::PreferredLanguage;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::HudIc;
use crate::hud::elevation::{self, Elevation};
use crate::hud::{Scale, px, tx};
use crate::services;
use crate::theme::{self, Palette, hex};
use crate::ui::{self, hex_alpha};

use super::{HudView, PILL_HAIRLINE};

const INSET: f32 = 3.;
const GAP: f32 = 2.;
pub(super) const CHIP_H: f32 = 26. + 2. * PILL_HAIRLINE;

fn cell_width(language: PreferredLanguage) -> f32 {
    if language == PreferredLanguage::En { 34. } else { 32. }
}

pub(super) fn chip_width(languages: impl Iterator<Item = PreferredLanguage>) -> f32 {
    let cells: f32 = languages.map(|language| cell_width(language) + GAP).sum();
    if cells == 0. { 0. } else { cells - GAP + 2. * (INSET + PILL_HAIRLINE) }
}

impl HudView {
    /// The same animated segmented control as the settings, sized for the HUD (dark track, gold sliding plate).
    /// Returns a builder that draws it at scale `k`, so the hover pop can grow it like a tooltip.
    pub(super) fn language_chip(
        &self,
        palette: &Palette,
        lang: Lang,
        cx: &mut Context<Self>,
    ) -> impl Fn(f32) -> Div + 'static {
        let cfg = self.st.cfg.get();
        let selection = cfg.general.hud_languages();
        let selected =
            selection.into_iter().position(|choice| choice == cfg.general.preferred_language).unwrap_or(usize::MAX);
        let mask = PreferredLanguage::ALL
            .iter()
            .enumerate()
            .fold(0, |mask, (i, language)| mask | (u8::from(cfg.general.hud_languages.contains(language)) << i));
        let mut p = *palette;
        p.seg = p.hud_bg;
        p.seg_on = theme::hud_lang_plate(&p);
        p.text = hex("#444139");
        p.muted = theme::hud_lang_muted(&p);
        let weak = cx.entity().downgrade();
        let palette = *palette;
        move |k| {
            let weak = weak.clone();
            // The plate springs whenever its target moves, and scaling moves it every frame; popping
            // frames draw under their own (quantized) ids so each one renders settled at its size.
            let id: SharedString = format!("hud-lang-{mask}-{}", (k * 100.).round()).into();
            let options: Vec<_> = cfg
                .general
                .hud_languages()
                .map(|language| ui::SegOption {
                    content: match language {
                        PreferredLanguage::Auto => ui::SegContent::Icon(HudIc::Auto),
                        PreferredLanguage::Ja => ui::SegContent::Text(lang.tr(Tr::HudLanguageJaShort).into()),
                        PreferredLanguage::En => ui::SegContent::Text("En".into()),
                        PreferredLanguage::Zh => ui::SegContent::Text(lang.tr(Tr::HudLanguageZhShort).into()),
                    },
                    width: Some(tx(cell_width(language)) * k),
                    tip: Some(language.name(lang).into()),
                })
                .collect();
            let cfg = cfg.clone();
            let seg = ui::seg_with(
                id,
                &p,
                &options,
                selected,
                Rc::new(move |i, _, app| {
                    if let Some(language) = cfg.general.hud_languages().nth(i) {
                        weak.update(app, |view, cx| view.pick_language(language, cx)).ok();
                    }
                }),
                ui::SegStyle {
                    h: tx(26.) * k,
                    label: tx(12.) * k,
                    cell_pad: tx(10.) * k,
                    inset: tx(INSET) * k,
                    gap: tx(GAP) * k,
                    hover: Some((hex_alpha(palette.text, 0.12), palette.text)),
                },
            );
            div()
                .rounded_full()
                .border(px(PILL_HAIRLINE))
                .bg(palette.hud_bg)
                .border_color(palette.hud_border)
                .shadow(elevation::surface_shadow(&palette, Scale::HUD, Elevation::Surface))
                .child(seg)
        }
    }

    fn pick_language(&mut self, preferred: PreferredLanguage, cx: &mut Context<Self>) {
        services::save_config(&self.st, |c| c.general.preferred_language = preferred);
        cx.notify();
    }
}

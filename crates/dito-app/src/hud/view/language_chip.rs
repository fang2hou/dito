//! The language selector above the capsule.

use std::rc::Rc;

use dito_core::config::PreferredLanguage;
use dito_core::i18n::Lang;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hud::elevation::{self, Elevation};
use crate::hud::{Scale, px, tx};
use crate::services;
use crate::theme::{self, PLAIN_BLACK, Palette, hex};
use crate::ui::{self, hex_alpha};

use super::{HudView, PILL_HAIRLINE};

impl HudView {
    /// The same animated segmented control as the settings, sized for the HUD (dark track, gold sliding plate).
    /// Returns a builder that draws it at scale `k`, so the hover pop can grow it like a tooltip.
    pub(super) fn language_chip(&self, palette: &Palette, cx: &mut Context<Self>) -> impl Fn(f32) -> Div + 'static {
        let selected = self.selected_language();
        let mut p = *palette;
        p.seg = p.hud_bg;
        p.seg_on = theme::hud_lang_plate(&p);
        p.text = hex(PLAIN_BLACK);
        p.muted = theme::hud_lang_muted(&p);
        let weak = cx.entity().downgrade();
        let palette = *palette;
        move |k| {
            let weak = weak.clone();
            // The plate springs whenever its target moves, and scaling moves it every frame; popping
            // frames draw under their own (quantized) ids so each one renders settled at its size.
            let id: SharedString =
                if k >= 0.999 { "hud-lang".into() } else { format!("hud-lang-pop-{}", (k * 100.).round()).into() };
            let seg = ui::seg_with(
                id,
                &p,
                &[SharedString::from("日本語"), SharedString::from("English"), SharedString::from("中文")],
                selected,
                Rc::new(move |i, _, app| {
                    weak.update(app, |view, cx| view.pick_language(i, cx)).ok();
                }),
                ui::SegStyle {
                    h: tx(26.) * k,
                    label: tx(12.) * k,
                    cell_pad: tx(10.) * k,
                    inset: tx(3.) * k,
                    gap: tx(2.) * k,
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

    /// The selector's segment for the language the HUD speaks now: the chosen one, or the app's own under Auto.
    fn selected_language(&self) -> usize {
        let cfg = self.st.cfg.get();
        let current = match cfg.general.preferred_language {
            PreferredLanguage::Ja => Lang::Ja,
            PreferredLanguage::En => Lang::En,
            PreferredLanguage::Zh => Lang::Zh,
            PreferredLanguage::Auto => cfg.general.lang(),
        };
        match current {
            Lang::Ja => 0,
            Lang::En => 1,
            Lang::Zh => 2,
        }
    }

    fn pick_language(&mut self, index: usize, cx: &mut Context<Self>) {
        let preferred = match index {
            0 => PreferredLanguage::Ja,
            1 => PreferredLanguage::En,
            _ => PreferredLanguage::Zh,
        };
        services::save_config(&self.st, |c| c.general.preferred_language = preferred);
        cx.notify();
    }
}

//! The language selector above the capsule.

use std::rc::Rc;

use dito_core::config::PreferredLanguage;
use dito_core::controller::Command;
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::{HudIc, Ic};
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

/// The two buttons of a command capsule stand to the right of the selector.
const BUTTON: f32 = 26.;
const BUTTON_GAP: f32 = 6.;

/// The selector with the polish and keyboard buttons to its right; the buttons alone when no language is offered.
pub(super) fn command_row_width(chip: f32) -> f32 {
    let buttons = 2. * BUTTON + BUTTON_GAP;
    if chip == 0. { buttons } else { chip + BUTTON_GAP + buttons }
}

/// A 26 pt circle in the selector's surface; hovered it turns to the command accent like an outlined action.
fn row_button(
    id: &'static str,
    glyph: Ic,
    tip: &str,
    k: f32,
    palette: &Palette,
    view: WeakEntity<HudView>,
    command: impl Fn() -> Command + 'static,
) -> impl IntoElement {
    let key = ElementId::from(id);
    ui::tip_label(key.clone(), SharedString::from(tip.to_string()));
    let target = f32::from(ui::is_hovered(&key));
    let p = *palette;
    let face = ui::tweened(ElementId::from((key.clone(), "face")), target, ui::HOVER_MS, move |t| {
        let ink = ui::mix_rgb(theme::hud_color(&p, "#444139", "#F7F5F0"), p.command_primary, t);
        div()
            .absolute()
            .inset_0()
            .rounded_full()
            .bg(ui::mix_rgb(p.hud_bg, p.command_soft, t))
            .border(px(PILL_HAIRLINE))
            .border_color(ui::mix_rgb(p.hud_border, p.command_primary, t))
            .flex()
            .items_center()
            .justify_center()
            .child(ui::icon(glyph, tx(14.) * k, ink))
    });
    div()
        .id(id)
        .relative()
        .size(px(BUTTON * k))
        .flex_none()
        .rounded_full()
        .cursor_pointer()
        .shadow(elevation::surface_shadow(palette, Scale::HUD, Elevation::Surface))
        .on_hover(ui::hover_tracker(key.clone()))
        .on_click(move |_, _, app| {
            view.update(app, |v, _| v.send(command())).ok();
        })
        .child(face)
        .child(ui::tip_above(palette, &key))
}

impl HudView {
    /// Returns a builder that draws the chip at scale `k`, so the hover pop can grow it like a tooltip.
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
        p.seg_on = if self.command_run() { theme::hud_command_lang_on(&p) } else { theme::hud_lang_plate(&p) };
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

    /// The command capsule's row: the selector, then the polish and keyboard buttons, drawn at scale `k` together.
    pub(super) fn command_row(
        &self,
        palette: &Palette,
        lang: Lang,
        cx: &mut Context<Self>,
    ) -> impl Fn(f32) -> Div + 'static {
        let weak = cx.entity().downgrade();
        let chip = self.language_chip(palette, lang, cx);
        let has_chip = chip_width(self.st.cfg.get().general.hud_languages()) > 0.;
        let palette = *palette;
        let polish = lang.tr(Tr::HudPolishSelection).to_string();
        let keyboard = lang.tr(Tr::HudTypeInstruction).to_string();
        move |k| {
            let mut row = div().flex().items_center().gap(px(BUTTON_GAP * k));
            if has_chip {
                row = row.child(chip(k));
            }
            row.child(row_button("hud-polish", Ic::Sparkles, &polish, k, &palette, weak.clone(), || {
                Command::PolishSelection
            }))
            .child(row_button(
                "hud-keyboard",
                Ic::Keyboard,
                &keyboard,
                k,
                &palette,
                weak.clone(),
                || Command::TypeInstruction,
            ))
        }
    }

    fn pick_language(&mut self, preferred: PreferredLanguage, cx: &mut Context<Self>) {
        services::save_config(&self.st, |c| c.general.preferred_language = preferred);
        cx.notify();
    }
}

use dito_core::i18n::Tr;
use dito_platform::hotkey::Shortcut;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::services;
use crate::settings::Ctx;
use crate::ui::{self, text};

use super::super::Onboarding;
use super::step_page::StepPage;
use crate::onboarding::components::{key_tile, keycap, keycap_waiting};

pub(super) fn page(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let p = &c.p;
    let (hero, caption) = if w.capturing { capture_prompt(c) } else { current_key(w, c, cx) };
    StepPage {
        title: c.t(Tr::OnboardingChooseShortcut),
        desc: c.t(Tr::OnboardingChooseShortcutDesc),
        body: div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(ui::card(p).bg(p.sub).py(px(26.)).items_center().gap(px(14.)).child(hero).child(caption))
            .child(
                ui::card(p)
                    .px(px(22.))
                    .py(px(18.))
                    .items_center()
                    .gap(px(14.))
                    .child(text(c.t(Tr::OnboardingQuickSetup), 13., FontWeight::BOLD, p.text))
                    .child(quick_keys(w, c, cx)),
            ),
    }
}

/// The three keys that set the shortcut with one click.
fn quick_keys(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> Div {
    let current = w.st.cfg.get().general.shortcut.clone();
    let mut keys = div().flex().gap(px(10.));
    for (i, id) in ["RightOption", "RightCommand", "Fn"].into_iter().enumerate() {
        let on = !w.capturing && id == current;
        let label = Shortcut::parse(id).map(|s| s.label(c.lang)).unwrap_or_default();
        let st = w.st.clone();
        keys = keys.child(key_tile(
            i,
            &c.p,
            label.into(),
            on,
            cx.listener(move |this, _, _, cx| {
                if this.capturing {
                    this.st.hotkeys.cancel_capture();
                }
                services::save_config(&st, |c| c.general.shortcut = id.to_string());
                st.hotkeys.set_shortcut(id);
                cx.notify();
            }),
        ));
    }
    keys
}

/// While the shortcut is being recorded: the waiting keycap and how to cancel.
fn capture_prompt(c: &Ctx) -> (AnyElement, Div) {
    let p = &c.p;
    (
        keycap_waiting(p, c.t(Tr::GeneralWaitingForInput)).into_any_element(),
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(text(c.t(Tr::OnboardingPressAnyKey), 14., FontWeight::MEDIUM, p.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .h(px(24.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .bg(p.card)
                            .border_1()
                            .border_color(p.border_strong)
                            .child(text("Esc", 12., FontWeight::BOLD, p.text).line_height(relative(1.))),
                    )
                    .child(text(c.t(Tr::OnboardingToCancel), 13., FontWeight::NORMAL, p.muted)),
            ),
    )
}

/// The current shortcut as a keycap that starts recording when clicked.
fn current_key(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (AnyElement, Div) {
    let p = &c.p;
    let current = Shortcut::parse(&w.st.cfg.get().general.shortcut)
        .map(|s| dito_platform::keymap::label(&s, c.lang))
        .unwrap_or_default();
    let hero_id = ElementId::from("shortcut-hero");
    let hero = div()
        .id(hero_id.clone())
        .relative()
        .cursor_pointer()
        .on_hover(ui::hover_tracker(hero_id.clone()))
        .on_click(cx.listener(|this, _, _, _| this.st.hotkeys.begin_capture()))
        .child(keycap(p, &current, true))
        .child(ui::hover_fill(&hero_id, Some(20.), ui::hex_alpha(p.seg, 0.6)))
        .into_any_element();
    (hero, div().child(text(c.t(Tr::OnboardingClickToRecord), 13., FontWeight::NORMAL, p.muted)))
}

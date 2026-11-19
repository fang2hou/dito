use dito_core::config::RecordMode;
use dito_core::i18n::Tr;
use dito_platform::hotkey::Shortcut;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::theme::hex;
use crate::ui::{self, text};

use super::super::Onboarding;
use super::step_page::StepPage;
use crate::onboarding::components::recommend_tag;
use crate::onboarding::demo;

pub(super) fn page(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let key =
        Shortcut::parse(&cfg.general.shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or_default();
    StepPage {
        title: c.t(Tr::OnboardingChooseMode),
        desc: c.t(Tr::OnboardingChooseModeDesc),
        body: div().w_full().flex().flex_col().gap(px(14.)).child(
            ui::card(p)
                .p(px(20.))
                .gap(px(18.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .gap(px(12.))
                        .child(mode_tile(c, cx, RecordMode::Toggle, w.mode))
                        .child(mode_tile(c, cx, RecordMode::Hold, w.mode)),
                )
                .child(demo::usage_demo(p, c.lang, w.mode, key)),
        ),
    }
}

fn mode_tile(c: &Ctx, cx: &mut Context<Onboarding>, mode: RecordMode, selected: RecordMode) -> Stateful<Div> {
    let p = &c.p;
    let on = mode == selected;
    let (i, name, desc, recommended) = match mode {
        RecordMode::Toggle => (0usize, Tr::GeneralToggle, Tr::OnboardingToggleDesc, true),
        RecordMode::Hold => (1, Tr::GeneralHold, Tr::OnboardingHoldDesc, false),
    };
    let tag = recommended.then(|| {
        let green = hex("#187760");
        let (background, ink) = if on { (green, ui::white()) } else { (hex("#EBF6F3"), green) };
        recommend_tag(c.t(Tr::OnboardingRecommended), background, ink)
    });
    ui::choice_surface(p, ("mode-tile", i), on)
        .flex_1()
        .px(px(20.))
        .py(px(16.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.mode = mode;
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(text(c.t(name), 16., FontWeight::BOLD, p.text))
                .children(tag),
        )
        .child(text(c.t(desc), 12., FontWeight::NORMAL, p.muted))
}

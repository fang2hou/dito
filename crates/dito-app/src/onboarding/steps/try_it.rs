use dito_core::config::RecordMode;
use dito_core::i18n::Tr;
use dito_platform::hotkey::Shortcut;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, text};

use super::super::Onboarding;
use super::step_page::StepPage;

pub(super) fn page(w: &Onboarding, c: &Ctx, _cx: &mut Context<Onboarding>) -> StepPage {
    let hold = w.mode == RecordMode::Hold;
    StepPage {
        title: c.t(Tr::OnboardingTryTitle),
        desc: if hold { c.t(Tr::OnboardingTryHoldDesc) } else { c.t(Tr::OnboardingTryToggleDesc) },
        body: div().w_full().h_full().flex().flex_col().gap(px(16.)).child(progress_row(w, c)).child(input_field(w, c)),
    }
}

fn step_labels(w: &Onboarding, c: &Ctx) -> [String; 3] {
    let cfg = w.st.cfg.get();
    let key =
        Shortcut::parse(&cfg.general.shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or_default();
    let (first, last) = match w.mode {
        RecordMode::Hold => (dito_core::i18n::fmt::hold_key(c.lang, &key), Tr::DemoRelease),
        RecordMode::Toggle => (dito_core::i18n::fmt::press_key(c.lang, &key), Tr::DemoPressAgain),
    };
    [first, c.t(Tr::DemoSpeak).to_string(), c.t(last).to_string()]
}

fn progress_row(w: &Onboarding, c: &Ctx) -> Div {
    let p = &c.p;
    let mut row = div().w_full().flex().gap(px(12.));
    for (i, label) in step_labels(w, c).into_iter().enumerate() {
        let done = w.seen[i];
        row = row.child(
            ui::card(p)
                .flex_1()
                .px(px(16.))
                .py(px(14.))
                .gap(px(8.))
                .child(
                    div()
                        .size(px(26.))
                        .rounded_full()
                        .bg(if done { p.success } else { p.seg })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon(Ic::Check, 14., if done { ui::white() } else { p.muted })),
                )
                .child(text(label, 13., FontWeight::MEDIUM, p.text)),
        );
    }
    row
}

fn input_field(w: &Onboarding, c: &Ctx) -> Div {
    let p = &c.p;
    let caret = || div().mt(px(5.)).w(px(2.)).h(px(24.)).flex_none().rounded(px(1.)).bg(p.accent);
    // An empty field shows the caret before the placeholder, as a real text field does.
    let input = div()
        .w_full()
        .flex_1()
        .px(px(18.))
        .py(px(14.))
        .rounded(px(12.))
        .bg(p.input_bg)
        .border_1()
        .border_color(p.accent)
        .flex()
        .flex_wrap()
        .items_start();
    let input = match &w.tried_text {
        Some(s) => input
            .child(text(s.clone(), 16., FontWeight::NORMAL, p.text).line_height(relative(1.7)))
            .child(caret().ml(px(2.))),
        None => input
            .child(caret().mr(px(2.)))
            .child(text(c.t(Tr::OnboardingTrySpeaking), 16., FontWeight::NORMAL, p.muted).line_height(relative(1.7))),
    };
    div()
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(text(c.t(Tr::OnboardingTextAppearsHere), 11., FontWeight::MEDIUM, p.muted))
        .child(input)
}

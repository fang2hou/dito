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
    let bounds = w.try_bounds.clone();
    let input = div()
        .w_full()
        .flex_1()
        .relative()
        .child(
            ui::text_area(
                &w.try_input,
                ui::TextAreaStyle {
                    bg: p.input_bg,
                    edge: Some(p.border_strong),
                    radius: 12.,
                    text_size: 16.,
                    line_height: 27.,
                    pad: [18., 16., 18., 16.],
                    fill: true,
                    reserve_bottom: 65.,
                },
            )
            .focus_ring(p.accent),
        )
        .child(canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| ()).absolute().inset_0());
    div()
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(text(c.t(Tr::OnboardingTextAppearsHere), 11., FontWeight::MEDIUM, p.muted))
        .child(input)
}

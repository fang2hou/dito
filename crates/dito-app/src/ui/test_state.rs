use dito_core::i18n::{Lang, Tr};
use dito_core::provider::TestOutcome;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::format::error_line;
use super::typography::text;
use crate::theme::Palette;

#[derive(Clone, Debug, PartialEq)]
pub enum TestState {
    Idle,
    Running,
    Ok(u64),
    Failed(String),
}

impl From<TestOutcome> for TestState {
    fn from(outcome: TestOutcome) -> Self {
        match outcome {
            TestOutcome::Ok { latency_ms } => Self::Ok(latency_ms),
            TestOutcome::Failed { kind, message } => Self::Failed(format!("{}: {message}", kind.code())),
        }
    }
}

pub fn test_status(p: &Palette, lang: Lang, state: &TestState) -> Div {
    let (label, bg, fg, weight) = match state {
        TestState::Idle => (lang.tr(Tr::ModelsUnchecked), p.seg, p.muted, FontWeight::NORMAL),
        TestState::Running => (lang.tr(Tr::ModelsChecking), p.seg, p.muted, FontWeight::NORMAL),
        TestState::Ok(_) => (lang.tr(Tr::ModelsTestOk), p.success_soft, p.success_text, FontWeight::MEDIUM),
        TestState::Failed(_) => {
            (lang.tr(Tr::ModelsConnectionError), p.button_danger_bg, p.button_danger_text, FontWeight::MEDIUM)
        }
    };
    let tag = div()
        .flex_none()
        .px(px(10.))
        .py(px(4.))
        .rounded(px(6.))
        .bg(bg)
        .child(text(label, 12., weight, fg).line_height(relative(1.5)));
    let mut status = div().flex().items_center().gap(px(12.)).child(tag);
    if let TestState::Ok(ms) = state {
        status = status.child(
            div()
                .flex()
                .items_baseline()
                .gap(px(6.))
                .child(text(lang.tr(Tr::ModelsLatency), 12., FontWeight::NORMAL, p.muted))
                .child(text(ms.to_string(), 13., FontWeight::MEDIUM, p.text))
                .child(text("ms", 12., FontWeight::NORMAL, p.muted)),
        );
    }
    status
}

/// Where the message of a failed test goes in [`test_summary`]: the cloud polish form sets it beside the tag
/// (`design:settings/cloud-models/failed-test`), the cloud speech form under it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorPlacement {
    Beside,
    Below,
}

pub fn test_summary(p: &Palette, lang: Lang, state: &TestState, error: ErrorPlacement) -> Div {
    let tag = test_status(p, lang, state);
    let message = match state {
        TestState::Failed(message) => Some(text(error_line(lang, message), 12., FontWeight::NORMAL, p.text)),
        _ => None,
    };
    match error {
        ErrorPlacement::Below => div().flex().flex_col().gap(px(6.)).child(tag).children(message),
        ErrorPlacement::Beside => div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(tag)
            .children(message.map(|message| message.flex_1().min_w(px(0.)))),
    }
}

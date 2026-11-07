//! Reusable wizard pieces: pills, keycaps, the stepper, the per-step art panel, and the footer.

use dito_core::i18n::{Lang, Tr};

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::theme::{Palette, hex};
use crate::ui::{self, BtnKind, BtnSize, cbk, text};

use super::{Onboarding, Step};

/// A primary pill that does nothing: the look of a button that is not available (the caller dims it).
pub(super) fn static_primary(p: &Palette, label: &str, lead: Option<Ic>, trail: Option<Ic>) -> Div {
    let mut b =
        div().h(px(40.)).px(px(22.)).flex().items_center().justify_center().gap(px(8.)).rounded_full().bg(p.primary);
    if let Some(ic) = lead {
        b = b.child(ui::icon(ic, 14., p.on_primary));
    }
    b = b.child(text(label.to_string(), 14., FontWeight::MEDIUM, p.on_primary).line_height(relative(1.)));
    if let Some(ic) = trail {
        b = b.child(ui::icon(ic, 14., p.on_primary));
    }
    b
}

pub(super) fn recommend_tag(label: SharedString, background: Hsla, ink: Hsla) -> Div {
    div()
        .px(px(11.))
        .py(px(3.))
        .rounded_full()
        .bg(background)
        .child(text(label, 12., FontWeight::MEDIUM, ink).line_height(relative(1.5)))
}

const STEP_LABELS: [Tr; 6] = [
    Tr::OnboardingStepPermissions,
    Tr::OnboardingStepHowTo,
    Tr::GeneralRecord,
    Tr::OnboardingStepEngine,
    Tr::OnboardingStepPrepare,
    Tr::OnboardingStepTry,
];

/// One preset-shortcut button.
pub(super) fn key_tile(
    i: usize,
    p: &Palette,
    label: SharedString,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let key_id = ElementId::from(("wizard-key", i));
    div()
        .id(key_id.clone())
        .w(px(96.))
        .h(px(44.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .cursor_pointer()
        .bg(if on { p.accent_soft } else { p.card })
        .relative()
        .border_1()
        .border_color(ui::transparent())
        .on_hover(ui::hover_tracker(key_id.clone()))
        .child(ui::hover_fill_if(&key_id, Some(12.), ui::hex_alpha(p.seg, 0.6), !on))
        .child(ui::ring_overlay(("wizard-key-ring", i), on, 12., p.border_strong, p.accent))
        .on_click(on_click)
        .child(ui::choice_label(&key_id, p, label, 15., on))
}

/// The big keycap while the wizard waits for the user to press a new shortcut.
pub(super) fn keycap_waiting(p: &Palette, label: SharedString) -> Div {
    div()
        .h(px(90.))
        .px(px(30.))
        .flex_none()
        .rounded(px(20.))
        .bg(p.accent_soft)
        .border_2()
        .border_color(p.accent)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.), px(6.)),
            blur_radius: px(16.),
            spread_radius: px(0.),
            inset: false,
        }])
        .flex()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .child(div().size(px(10.)).rounded_full().bg(p.danger))
        .child(text(label, 24., FontWeight::BOLD, p.accent_text).line_height(relative(1.)))
}

pub(super) fn keycap(p: &Palette, label: &str, big: bool) -> Div {
    let (w, h, fs) = if big { (150., 90., 28.) } else { (56., 36., 15.) };
    div()
        .min_w(px(w))
        .px(px(if big { 30. } else { 8. }))
        .h(px(h))
        .flex_none()
        .rounded(px(if big { 20. } else { 10. }))
        .bg(p.card)
        .border_1()
        .border_color(p.border_strong)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.), px(if big { 6. } else { 2. })),
            blur_radius: px(if big { 16. } else { 4. }),
            spread_radius: px(0.),
            inset: false,
        }])
        .flex()
        .items_center()
        .justify_center()
        .child(text(label.to_string(), fs, FontWeight::BOLD, p.text).line_height(relative(1.)))
}

pub(super) fn stepper(step: Step, p: &Palette, lang: Lang) -> Div {
    let cur = step.number();
    let mut row = div().flex().items_center().gap(px(10.));
    for (i, label) in STEP_LABELS.iter().enumerate() {
        let n = i + 1;
        let active = n <= cur;
        row = row.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .size(px(18.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if active { p.primary } else { p.seg })
                        .child(
                            text(n.to_string(), 10., FontWeight::BOLD, if active { p.on_primary } else { p.muted })
                                .line_height(relative(1.)),
                        ),
                )
                .child(text(
                    lang.tr(*label),
                    11.,
                    if n == cur { FontWeight::BOLD } else { FontWeight::NORMAL },
                    if n == cur { p.text } else { p.muted },
                )),
        );
        if n < STEP_LABELS.len() {
            row = row.child(div().w(px(14.)).h(px(1.)).bg(p.border_strong));
        }
    }
    row
}

/// Right-hand art, exported from the design's per-step `deco-panel` frames (`assets/onboarding`).
/// The artwork keeps its design size and is centre-cropped when the window is shorter than the panel.
pub(super) fn art(step: Step, p: &Palette) -> Div {
    let name = match step {
        Step::Welcome => return div(),
        Step::Permissions => "perm",
        Step::Usage => "usage",
        Step::Shortcut => "key",
        Step::Engine => "engine",
        Step::Prepare => "prepare",
        Step::Try => "try",
    };
    let theme = if p.dark { "dark" } else { "light" };
    div()
        .w(px(300.))
        .h_full()
        .flex_none()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(if p.dark { hex("#5B5340") } else { hex("#FFF9EB") })
        .child(img(format!("onboarding/{name}-{theme}.png")).w(px(300.)).h(px(718.)).flex_none())
}

pub(super) fn footer(
    step: Step,
    c: &crate::settings::Ctx,
    cx: &mut Context<Onboarding>,
    next_label: SharedString,
    can_next: bool,
) -> Div {
    let p = &c.p;
    // 続ける ends with an arrow, 完了 starts with a check; はじめる is plain.
    let (lead, trail) = match step {
        Step::Welcome => (None, None),
        Step::Try => (Some(Ic::Check), None),
        _ => (None, Some(Ic::ArrowRight)),
    };
    let next = if can_next {
        ui::button_with(
            "wizard-next",
            p,
            &next_label,
            BtnKind::Primary,
            BtnSize::Large,
            lead,
            trail,
            cbk(cx, |this: &mut Onboarding, _, cx| this.next(cx)),
        )
        .into_any_element()
    } else {
        ui::reset_hover(&ElementId::from("wizard-next"));
        static_primary(p, &next_label, lead, trail).opacity(0.45).into_any_element()
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(if step == Step::Welcome {
            div()
        } else {
            div().child(ui::button_with(
                "wizard-back",
                p,
                &c.t(Tr::OnboardingBack),
                BtnKind::Secondary,
                BtnSize::Large,
                Some(Ic::ArrowLeft),
                None,
                cbk(cx, |this: &mut Onboarding, _, cx| this.back(cx)),
            ))
        })
        .child(next)
}

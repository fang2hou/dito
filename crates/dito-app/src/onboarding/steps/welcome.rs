use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, BtnSize, cbk};

use super::super::Onboarding;

const WELCOME_LOGO_H: f32 = 103.;
const WELCOME_TAG_H: f32 = 26.;
const WELCOME_TAG_GAP: f32 = 14.;
const WELCOME_GAP: f32 = 56.;

/// The health-dot is green in every theme.
fn welcome_logo(p: &Palette, ink: Hsla) -> Div {
    div()
        .h(px(WELCOME_LOGO_H))
        .flex()
        .items_end()
        .px(px(24.))
        .child(ui::tracked("DITO", 68., FontWeight::SEMIBOLD, ink, 0.14))
        .child(div().size(px(24.)).flex_none().rounded_full().bg(p.success).mb(px(12.)))
}

pub(in crate::onboarding) fn page(_w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> Div {
    let p = &c.p;
    let tagline = c.t(Tr::OnboardingTagline);
    let cjk = ui::is_cjk(&tagline);
    let tagline = div().h(px(WELCOME_TAG_H)).flex().items_center().whitespace_nowrap().child(ui::text_tracked(
        &tagline,
        if cjk { 19. } else { 18. },
        FontWeight::NORMAL,
        p.muted,
        0.08,
    ));
    let button = ui::button_with(
        "wizard-start",
        p,
        &c.t(Tr::OnboardingGetStarted),
        BtnKind::Primary,
        BtnSize::Hero,
        None,
        None,
        cbk(cx, |this: &mut Onboarding, _, cx| this.next(cx)),
    );
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(welcome_logo(p, p.text))
        .child(div().h(px(WELCOME_TAG_GAP)))
        .child(tagline)
        .child(div().h(px(WELCOME_GAP)))
        .child(button)
}

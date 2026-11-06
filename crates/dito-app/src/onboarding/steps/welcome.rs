use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, text};

use super::super::Onboarding;

const WELCOME_BTN_H: f32 = 50.;
const WELCOME_LOGO_H: f32 = 103.;
const WELCOME_TAG_H: f32 = 26.;
const WELCOME_TAG_GAP: f32 = 14.;
const WELCOME_GAP: f32 = 56.;

fn welcome_button_w(label: &str) -> f32 {
    (ui::text_width(label, 15., FontWeight::MEDIUM) + 80.).ceil().max(142.)
}

/// The wordmark with the health-dot (green in every theme); `ink` is the wordmark's color.
fn welcome_logo(p: &Palette, ink: Hsla) -> Div {
    div()
        .h(px(WELCOME_LOGO_H))
        .flex()
        .items_end()
        .px(px(24.))
        .child(ui::tracked("DITO", 68., FontWeight::SEMIBOLD, ink, 0.14))
        .child(div().size(px(24.)).flex_none().rounded_full().bg(p.success).mb(px(12.)))
}

/// The logo, the tagline and one button.
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
    let btn_id = ElementId::from("wizard-start");
    let tint = if p.primary.l < 0.5 { hsla(0., 0., 1., 0.24) } else { hsla(0., 0., 0., 0.16) };
    let button = div()
        .id(btn_id.clone())
        .relative()
        .w(px(welcome_button_w(&c.t(Tr::OnboardingGetStarted))))
        .h(px(WELCOME_BTN_H))
        .rounded_full()
        .bg(p.primary)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_hover(ui::hover_tracker(btn_id.clone()))
        .child(ui::hover_fill(&btn_id, None, tint))
        .child(text(c.t(Tr::OnboardingGetStarted), 15., FontWeight::MEDIUM, p.on_primary).line_height(relative(1.)))
        .on_click(cx.listener(|this, _, _, cx| this.next(cx)));
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

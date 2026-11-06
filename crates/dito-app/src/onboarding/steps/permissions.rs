use dito_core::i18n::Tr;
use dito_platform::Status;
use dito_platform::permissions;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, cbk, text};

use super::step_page::StepPage;
use crate::onboarding::{Onboarding, PERMS};

pub(super) fn page(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let rows = [
        (Ic::Mic, c.t(Tr::GeneralMicrophone), c.t(Tr::OnboardingCardMic)),
        (Ic::Zap, c.t(Tr::StatusAccessibility), c.t(Tr::OnboardingCardAccessibility)),
    ];
    let mut cards = div().w_full().flex().gap(px(16.));
    for (i, (icon, name, desc)) in rows.into_iter().enumerate() {
        cards = cards.child(permission_card(w, c, cx, i, icon, name, desc));
    }
    StepPage { title: c.t(Tr::OnboardingAllowPermissions), desc: c.t(Tr::OnboardingPermissionsDesc), body: cards }
}

fn permission_card(
    w: &Onboarding,
    c: &Ctx,
    cx: &mut Context<Onboarding>,
    i: usize,
    icon: Ic,
    name: SharedString,
    desc: SharedString,
) -> Div {
    let p = &c.p;
    ui::card(p)
        .flex_1()
        .h(px(236.))
        .p(px(26.))
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(
                    div()
                        .size(px(56.))
                        .rounded(px(18.))
                        .bg(p.seg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon(icon, 28., p.text)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(text(name, 16., FontWeight::BOLD, p.text))
                        .child(ui::muted12(p, desc).text_size(px(13.))),
                ),
        )
        .child(permission_action(w, c, cx, i))
}

fn permission_action(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>, i: usize) -> AnyElement {
    let p = &c.p;
    if w.perms[i] == Status::Granted {
        return div().flex().child(ui::status_tag(p, true, c.t(Tr::StatusAllowed))).into_any_element();
    }
    let perm = PERMS[i];
    // The microphone is granted through the system prompt; accessibility only in System Settings.
    let (label, lead) =
        if i == 0 { (c.t(Tr::GeneralAllow), None) } else { (c.t(Tr::StatusOpenSystemSettings), Some(Ic::External)) };
    ui::button(
        SharedString::from(format!("allow-{i}")),
        p,
        &label,
        BtnKind::Secondary,
        lead,
        cbk(cx, move |_: &mut Onboarding, _, _| permissions::request(perm)),
    )
    .w_full()
    .into_any_element()
}

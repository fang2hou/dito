use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
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
        (Ic::Headphones, c.t(Tr::StatusSystemAudio), c.t(Tr::OnboardingCardSystemAudio)),
    ];
    let mut first_row = div().w_full().flex().gap(px(16.));
    let mut last_row = div().w_full().flex();
    for (i, (icon, name, desc)) in rows.into_iter().enumerate() {
        let card = permission_card(w, c, cx, i, icon, name, desc);
        if i < 2 {
            first_row = first_row.child(card);
        } else {
            last_row = last_row.child(card);
        }
    }
    let cards = div().w_full().flex().flex_col().gap(px(15.)).child(first_row).child(last_row);
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
        .h(px(194.))
        .p(px(20.))
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .child(
                            div()
                                .size(px(56.))
                                .flex_none()
                                .rounded(px(18.))
                                .bg(p.seg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ui::icon(icon, 30., p.text)),
                        )
                        .child(text(name, 15., FontWeight::MEDIUM, p.text).whitespace_normal()),
                )
                .child(ui::muted12(p, desc).text_size(px(13.))),
        )
        .child(permission_action(w, c, cx, i))
}

fn permission_action(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>, i: usize) -> AnyElement {
    let p = &c.p;
    if w.perms[i] == Status::Granted {
        return div().flex().child(ui::status_tag(p, true, c.t(Tr::StatusAllowed))).into_any_element();
    }
    let perm = PERMS[i];
    // The microphone and system audio are granted through the system prompt; accessibility only in System
    // Settings.
    let (label, lead) = match perm {
        Permission::Microphone | Permission::SystemAudio => (c.t(Tr::GeneralAllow), None),
        Permission::Accessibility => (c.t(Tr::StatusOpenSystemSettings), Some(Ic::External)),
    };
    div()
        .flex()
        .child(ui::button(
            SharedString::from(format!("allow-{i}")),
            p,
            &label,
            BtnKind::Secondary,
            lead,
            cbk(cx, move |_: &mut Onboarding, _, _| permissions::request(perm)),
        ))
        .into_any_element()
}

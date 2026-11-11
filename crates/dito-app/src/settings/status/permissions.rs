use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::*;

use super::page::StatusPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, StatusTone, cbk, text};

pub(super) const PERMS: [Permission; 2] = [Permission::Microphone, Permission::Accessibility];

pub(super) fn read_perms() -> [Status; 2] {
    PERMS.map(permissions::status)
}

struct PermissionInfo {
    icon: Ic,
    name: SharedString,
    description: SharedString,
}

impl StatusPage {
    pub(super) fn permissions_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = c.p;
        // Permissions poll on their own while the page is open; the only manual recheck is
        // the page-wide button above, so the card header carries no button of its own.
        let infos = [
            PermissionInfo { icon: Ic::Mic, name: c.t(Tr::GeneralMicrophone), description: c.t(Tr::OnboardingCardMic) },
            PermissionInfo {
                icon: Ic::Zap,
                name: c.t(Tr::StatusAccessibility),
                description: c.t(Tr::OnboardingCardPolish),
            },
        ];
        let mut card = ui::card(&p);
        let mut order: Vec<usize> = (0..infos.len()).collect();
        order.sort_by_key(|&i| self.perms[i] == Status::Granted);
        for (k, &i) in order.iter().enumerate() {
            let mut row = self.permission_row(c, cx, i, &infos[i]);
            if k + 1 < infos.len() {
                row = row.border_b_1().border_color(p.border);
            }
            card = card.child(row);
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(text(c.t(Tr::ModelsSystemPermissions), 13., FontWeight::BOLD, p.muted))
            .child(card)
    }

    fn permission_row(&self, c: &Ctx, cx: &mut Context<Self>, index: usize, info: &PermissionInfo) -> Div {
        let p = c.p;
        let granted = self.perms[index] == Status::Granted;
        let permission = PERMS[index];
        let mut right = div().flex().items_center().gap(px(12.)).flex_none();
        if !granted {
            right = right.child(ui::button(
                SharedString::from(format!("open-perm-{index}")),
                &p,
                &c.t(Tr::StatusOpenSystemSettings),
                BtnKind::Secondary,
                Some(Ic::External),
                cbk(cx, move |_: &mut Self, _, _| permissions::request(permission)),
            ));
        }
        right = right.child(ui::status_chip(
            &p,
            if granted { c.t(Tr::StatusAllowed) } else { c.t(Tr::StatusNotAllowed) },
            if granted { StatusTone::Success } else { StatusTone::Danger },
        ));
        div()
            .w_full()
            .px(px(26.))
            .py(px(18.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .size(px(44.))
                            .rounded(px(14.))
                            .bg(p.seg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(ui::icon(info.icon, 20., p.text)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(info.name.clone(), 14., FontWeight::MEDIUM, p.text))
                            .child(ui::muted12(&p, info.description.clone())),
                    ),
            )
            .child(right)
    }
}

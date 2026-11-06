use dito_core::i18n::Tr;
use dito_platform::{Permission, permissions};
use gpui_kit::*;

use super::page::GeneralPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, cbk, text};

impl GeneralPage {
    pub(super) fn banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        if self.accessibility_ok && self.st.hotkeys.available() {
            return None;
        }
        let p = &c.p;
        let (bg, border, fg) = if p.dark {
            (ui::hex_alpha(p.danger, 0.12), ui::hex_alpha(p.danger, 0.5), p.text)
        } else {
            (p.danger_soft, p.danger, p.danger_text)
        };
        Some(
            div()
                .w_full()
                .pl(px(20.))
                .pr(px(12.))
                .py(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(14.))
                .rounded(px(16.))
                .bg(bg)
                .border_1()
                .border_color(border)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(ui::icon(Ic::Alert, 18., if p.dark { p.danger } else { p.danger_text }))
                        .child(text(c.t(Tr::StatusNeedAccessibility), 13., FontWeight::MEDIUM, fg)),
                )
                .child(ui::button(
                    "allow",
                    p,
                    &c.t(Tr::GeneralAllow),
                    if p.dark { BtnKind::Secondary } else { BtnKind::Primary },
                    None,
                    cbk(cx, |_, _, _| permissions::request(Permission::Accessibility)),
                )),
        )
    }
}

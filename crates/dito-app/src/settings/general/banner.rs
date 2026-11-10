use dito_core::i18n::Tr;
use dito_platform::{Permission, permissions};
use gpui_kit::*;

use super::page::GeneralPage;
use crate::settings::Ctx;
use crate::ui::{self, BannerAction, BtnKind, cbk};

impl GeneralPage {
    pub(super) fn banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        if self.accessibility_ok && self.st.hotkeys.available() {
            return None;
        }
        Some(ui::banner(
            &c.p,
            c.t(Tr::StatusNeedAccessibility),
            Some(BannerAction {
                id: "allow".into(),
                label: c.t(Tr::GeneralAllow),
                kind: if c.p.dark { BtnKind::Secondary } else { BtnKind::Primary },
                on_click: cbk(cx, |_, _, _| permissions::request(Permission::Accessibility)),
            }),
        ))
    }
}

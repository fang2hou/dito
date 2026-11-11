use dito_core::i18n::Tr;
use gpui_kit::*;

use super::page::GeneralPage;
use crate::assets::Ic;
use crate::services;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, cbk};

impl GeneralPage {
    pub(super) fn setup_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        // Testing aid.
        let restart_wizard = ui::button(
            "restart-wizard",
            &p,
            &c.t(Tr::OnboardingRunAgain),
            BtnKind::Secondary,
            Some(Ic::Reset),
            cbk(cx, |this: &mut Self, window, cx| {
                services::save_config(&this.st, |c| c.onboarded = false);
                let st = this.st.clone();
                window.remove_window();
                cx.defer(move |cx| crate::onboarding::restart(st, cx));
            }),
        );

        ui::group(
            &p,
            c.t(Tr::OnboardingInitialSetup),
            vec![ui::row(&p, c.t(Tr::OnboardingSetupWizard), restart_wizard, true)],
        )
    }
}

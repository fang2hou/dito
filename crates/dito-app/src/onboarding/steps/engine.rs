use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, text};

use super::super::Onboarding;
use super::step_page::StepPage;
use crate::onboarding::components::recommend_tag;

pub(super) fn page(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let cfg = w.st.cfg.get();
    let local_points = vec![
        (Ic::Lock, c.t(Tr::OnboardingLocalCardPrivacy)),
        (
            Ic::Download,
            dito_core::i18n::fmt::needs_download_about(
                c.lang,
                &ui::fmt_bytes(w.models_bytes(&[&cfg.asr.local_model, &cfg.polish.engine.local_model])),
            )
            .into(),
        ),
        (Ic::Zap, c.t(Tr::OnboardingLocalCardOffline)),
    ];
    let provider_points = vec![
        (Ic::Globe, c.t(Tr::OnboardingProviderCardProviders)),
        (Ic::Lock, c.t(Tr::OnboardingProviderCardNeedsNetwork)),
        (Ic::Cpu, c.t(Tr::OnboardingCardLight)),
    ];
    StepPage {
        title: c.t(Tr::OnboardingRunLocally),
        desc: c.t(Tr::OnboardingChooseEngineDesc),
        body: div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(option(c, cx, 0, c.t(Tr::ModelsLocal), true, local_points, w.local))
            .child(option(c, cx, 1, c.t(Tr::ModelsProvider), false, provider_points, !w.local)),
    }
}

fn option(
    c: &Ctx,
    cx: &mut Context<Onboarding>,
    i: usize,
    title: SharedString,
    recommended: bool,
    points: Vec<(Ic, SharedString)>,
    on: bool,
) -> Stateful<Div> {
    let p = &c.p;
    let card_id = ElementId::from(("engine-opt", i));
    let mut card = div()
        .id(card_id.clone())
        .w_full()
        .p(px(22.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .rounded(px(16.))
        .cursor_pointer()
        .bg(if on { p.accent_soft } else { p.card })
        .relative()
        .border_1()
        .border_color(ui::transparent())
        .on_hover(ui::hover_tracker(card_id.clone()))
        .child(ui::hover_fill_if(&card_id, Some(16.), ui::hex_alpha(p.seg, 0.6), !on))
        .child(ui::ring_overlay(("engine-ring", i), on, 16., p.border, p.accent))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.local = i == 0;
            cx.notify();
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(ui::radio(p, on))
                .child(text(title, 16., FontWeight::BOLD, p.text))
                .children(recommended.then(|| recommend_tag(c.t(Tr::OnboardingRecommended), p.success, ui::white()))),
        );
    for (ic, s) in points {
        card =
            card.child(div().flex().items_center().gap(px(10.)).child(ui::icon(ic, 16., p.accent_text)).child(text(
                s,
                13.,
                FontWeight::NORMAL,
                p.text,
            )));
    }
    card
}

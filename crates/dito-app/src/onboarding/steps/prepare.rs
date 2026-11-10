use dito_core::i18n::Tr;
use dito_local::{ModelState, Snapshot};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, BtnSize, TestState, cbk, text};

use super::step_page::StepPage;
use super::tier::TIERS;
use crate::onboarding::components::static_primary;
use crate::onboarding::{Onboarding, PreparePhase};

pub(super) fn local(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let snap: Snapshot = w.st.models.subscribe().borrow().clone();
    let ids = [cfg.asr.local_model.clone(), cfg.polish.engine.local_model.clone()];
    let phase = w.prepare_phase();
    let col = div().w_full().flex().flex_col().gap(px(16.)).child(tier_cards(w, c, cx, phase));
    let mut rows = ui::card(p).p(px(22.)).gap(px(16.));
    for (role, id) in [c.t(Tr::ModelsDictation), c.t(Tr::ModelsPolishShort)].into_iter().zip(ids) {
        rows = rows.child(model_progress(w, c, &role, &id, &snap));
    }
    if phase != PreparePhase::Ready {
        rows = rows.child(prepare_footer(w, c, cx, phase));
    }
    StepPage { title: c.t(Tr::OnboardingPrepareLocal), desc: c.t(Tr::OnboardingPrepareDesc), body: col.child(rows) }
}

fn tier_cards(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>, phase: PreparePhase) -> Div {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let current = TIERS.iter().position(|t| t.asr == cfg.asr.local_model && t.polish == cfg.polish.engine.local_model);
    let locked = phase == PreparePhase::Downloading;
    let mut tiers = div().w_full().flex().gap(px(12.));
    if locked {
        tiers = tiers.opacity(0.55);
    }
    for (i, tier) in TIERS.iter().enumerate() {
        let on = current == Some(i);
        let card_id = ElementId::from(("wizard-tier", i));
        tiers = tiers.child(
            div()
                .id(card_id.clone())
                .flex_1()
                .min_w_0()
                .p(px(18.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(16.))
                .when(!locked, |d| d.cursor_pointer())
                .bg(if on { p.accent_soft } else { p.card })
                .relative()
                .border_1()
                .border_color(ui::transparent())
                .on_hover(ui::hover_tracker(card_id.clone()))
                .child(ui::hover_fill_if(&card_id, Some(16.), ui::hex_alpha(p.seg, 0.6), !on))
                .child(ui::ring_overlay(("wizard-tier-ring", i), on, 16., p.border, p.accent))
                .on_click(cx.listener(move |this, _, _, cx| this.pick_tier(i, cx)))
                .child(div().flex().items_center().gap(px(8.)).child(ui::radio(p, on)).child(text(
                    c.t(tier.name),
                    15.,
                    FontWeight::BOLD,
                    p.text,
                )))
                .child(text(ui::fmt_bytes(w.models_bytes(&[tier.asr, tier.polish])), 13., FontWeight::MEDIUM, p.text))
                .child(text(c.t(tier.desc), 12., FontWeight::NORMAL, p.muted).whitespace_nowrap()),
        );
    }
    tiers
}

/// One model's name and download note above its progress bar.
fn model_progress(w: &Onboarding, c: &Ctx, role: &str, id: &str, snap: &Snapshot) -> Div {
    let p = &c.p;
    let name = w.st.models.spec(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string());
    let name = format!("{role} {name}");
    let state = snap.get(id).cloned().unwrap_or(ModelState::NotInstalled);
    let (frac, note) = match &state {
        ModelState::Installed => (1.0, c.t(Tr::OnboardingReady).to_string()),
        ModelState::Downloading { done, total, .. }
        | ModelState::Verifying { done, total }
        | ModelState::Paused { done, total } => {
            (*done as f32 / (*total).max(1) as f32, format!("{} / {}", ui::fmt_bytes(*done), ui::fmt_bytes(*total)))
        }
        ModelState::Failed { message, .. } => (0.0, format!("{}: {message}", c.t(Tr::ModelsFailed))),
        ModelState::NotInstalled => (0.0, c.t(Tr::OnboardingWaiting).to_string()),
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .w_full()
                .flex()
                .justify_between()
                .child(text(name, 14., FontWeight::MEDIUM, p.text))
                .child(ui::muted12(p, note)),
        )
        .child(ui::progress_bar(p, frac))
}

/// The download size note with the button that starts or retries the preparation, or a disabled one while it runs.
fn prepare_footer(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>, phase: PreparePhase) -> Div {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let (note, action) = match phase {
        PreparePhase::Downloading => (
            c.t(Tr::OnboardingDownloading).to_string(),
            static_primary(p, &c.t(Tr::OnboardingPreparing), None, None).opacity(0.45).into_any_element(),
        ),
        _ => {
            let (label, ic) = if phase == PreparePhase::Failed {
                (c.t(Tr::HistoryRetry), Ic::Refresh)
            } else {
                (c.t(Tr::OnboardingStartPreparing), Ic::Download)
            };
            (
                dito_core::i18n::fmt::downloads_model(
                    c.lang,
                    &ui::fmt_bytes(w.models_bytes(&[&cfg.asr.local_model, &cfg.polish.engine.local_model])),
                ),
                ui::button_with(
                    "wizard-prepare",
                    p,
                    &label,
                    BtnKind::Primary,
                    BtnSize::Large,
                    Some(ic),
                    None,
                    cbk(cx, |this: &mut Onboarding, _, cx| this.start_prepare(cx)),
                )
                .into_any_element(),
            )
        }
    };
    div()
        .w_full()
        .pt(px(16.))
        .border_t_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .justify_between()
        .child(text(note, 13., FontWeight::NORMAL, p.muted))
        .child(action)
}

pub(super) fn provider(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> StepPage {
    let p = &c.p;
    let (l1, l2) = (c.t(Tr::ModelsBaseUrl), c.t(Tr::ModelsModelId));
    let mut form = ui::card(p).p(px(22.)).gap(px(16.)).child(cloud_asr_tiles(w, c, cx)).child(ui::field(
        p,
        l1,
        ui::text_input(p, &w.endpoint, None),
    ));
    form = form.child(
        div()
            .w_full()
            .flex()
            .gap(px(16.))
            .child(div().flex_1().child(ui::field(p, l2, ui::text_input(p, &w.deployment, None))))
            .child(div().flex_1().child(ui::field(
                p,
                c.t(Tr::ModelsApiKey),
                ui::text_input(p, &w.key, Some(Ic::Lock)),
            ))),
    );
    let ready = !w.endpoint.read(cx).value().trim().is_empty() && !w.deployment.read(cx).value().trim().is_empty();
    form = form.child(
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pt(px(16.))
            .border_t_1()
            .border_color(p.border)
            .child(ui::button(
                "wizard-config",
                p,
                &c.t(Tr::ModelsConnectionSettings),
                BtnKind::Secondary,
                None,
                cbk(cx, |this: &mut Onboarding, w, cx| this.open_connection_settings(w, cx)),
            ))
            .child(test_control(w, c, cx, ready)),
    );
    if let TestState::Failed(message) = &w.test {
        form = form.child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text));
    }
    StepPage { title: c.t(Tr::OnboardingConnectProvider), desc: "".into(), body: form }
}

fn cloud_asr_tiles(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> Div {
    let p = &c.p;
    let mut row = div().w_full().flex().gap(px(10.));
    let profiles = w.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default().profiles;
    for profile in &profiles {
        let on = profile.id == w.cloud_asr_id;
        let id = SharedString::from(profile.id.clone());
        let card_id = ElementId::from((ElementId::from("wizard-cloud-asr"), id.clone()));
        let ring_id = ElementId::from((ElementId::from("wizard-cloud-asr-ring"), id.clone()));
        row = row.child(
            div()
                .id(card_id.clone())
                .flex_1()
                .h(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.))
                .cursor_pointer()
                .bg(if on { p.accent_soft } else { p.card })
                .relative()
                .border_1()
                .border_color(ui::transparent())
                .on_hover(ui::hover_tracker(card_id.clone()))
                .child(ui::hover_fill_if(&card_id, Some(14.), ui::hex_alpha(p.seg, 0.6), !on))
                .child(ui::ring_overlay(ring_id, on, 14., p.border_strong, p.accent))
                .on_click(cx.listener(move |this, _, window, cx| this.select_cloud_asr(&id, window, cx)))
                .child(ui::choice_label(&card_id, p, profile.name.clone().into(), 13., on)),
        );
    }
    row
}

/// The connection test's state as a control: a connected badge, the running note, or the button (dimmed until the
/// endpoint and model are filled in).
fn test_control(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>, ready: bool) -> AnyElement {
    let p = &c.p;
    match &w.test {
        TestState::Ok(_) => div()
            .h(px(36.))
            .px(px(16.))
            .rounded_full()
            .bg(p.success_soft)
            .flex()
            .items_center()
            .gap(px(8.))
            .child(ui::icon(Ic::Check, 14., p.success_text))
            .child(text(c.t(Tr::ModelsConnected), 13., FontWeight::MEDIUM, p.success_text))
            .into_any_element(),
        TestState::Running => text(c.t(Tr::ModelsChecking), 13., FontWeight::MEDIUM, p.muted).into_any_element(),
        _ if ready => ui::button(
            "wizard-test",
            p,
            &c.t(Tr::ModelsTestConnection),
            BtnKind::Secondary,
            None,
            cbk(cx, |this: &mut Onboarding, _, cx| this.run_test(cx)),
        )
        .into_any_element(),
        _ => div()
            .h(px(36.))
            .px(px(18.))
            .rounded_full()
            .bg(p.seg)
            .flex()
            .items_center()
            .child(text(c.t(Tr::ModelsTestConnection), 13., FontWeight::MEDIUM, p.muted))
            .into_any_element(),
    }
}

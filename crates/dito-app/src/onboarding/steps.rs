//! The content of each step: one function per page, plus the body dispatcher that picks it.

use dito_core::config::{DEFAULT_ASR_LOCAL, DEFAULT_POLISH_LOCAL, RecordMode};
use dito_core::i18n::Tr;
use dito_local::{ModelState, Snapshot};
use dito_platform::Status;
use dito_platform::hotkey::Shortcut;
use dito_platform::permissions;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, BtnSize, TestState, cbk, text};

use super::components::{key_tile, keycap, keycap_waiting, recommend_tag, static_primary};
use super::{Onboarding, PERMS, PreparePhase, Step};

/// Local model bundles offered on the prepare step; the last one is the app default.
pub(super) struct Tier {
    pub(super) name: Tr,
    pub(super) desc: Tr,
    pub(super) asr: &'static str,
    pub(super) polish: &'static str,
}

pub(super) const TIERS: [Tier; 3] = [
    Tier {
        name: Tr::OnboardingTierLight,
        desc: Tr::OnboardingTierLightDesc,
        asr: "sensevoice-small",
        polish: "qwen3.5-2b-instruct",
    },
    Tier {
        name: Tr::OnboardingTierStandard,
        desc: Tr::OnboardingTierStandardDesc,
        asr: "qwen3-asr-0.6b",
        polish: "qwen3.5-4b-instruct",
    },
    Tier {
        name: Tr::OnboardingTierHigh,
        desc: Tr::OnboardingTierHighDesc,
        asr: DEFAULT_ASR_LOCAL,
        polish: DEFAULT_POLISH_LOCAL,
    },
];

/// A step's page content: its title, description and body column.
pub(super) fn body(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    match w.step {
        // The welcome page is drawn by `welcome`, not as a titled page.
        Step::Welcome => (SharedString::default(), SharedString::default(), div()),
        Step::Permissions => permissions(w, c, cx),
        Step::Usage => usage(w, c, cx),
        Step::Shortcut => shortcut(w, c, cx),
        Step::Engine => engine(w, c, cx),
        Step::Prepare if w.local => prepare_local(w, c, cx),
        Step::Prepare => prepare_provider(w, c, cx),
        Step::Try => try_step(w, c, cx),
    }
}

fn permissions(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let rows = [
        (Ic::Mic, c.t(Tr::GeneralMicrophone), c.t(Tr::OnboardingCardMic)),
        (Ic::Zap, c.t(Tr::StatusAccessibility), c.t(Tr::OnboardingCardAccessibility)),
    ];
    let mut cards = div().w_full().flex().gap(px(16.));
    for (i, (ic, name, desc)) in rows.into_iter().enumerate() {
        let ok = w.perms[i] == Status::Granted;
        let perm = PERMS[i];
        // The microphone is granted through the system prompt; accessibility only in System Settings.
        let (label, lead) = if i == 0 {
            (c.t(Tr::GeneralAllow), None)
        } else {
            (c.t(Tr::StatusOpenSystemSettings), Some(Ic::External))
        };
        let action = if ok {
            div().flex().child(ui::status_tag(p, true, c.t(Tr::StatusAllowed))).into_any_element()
        } else {
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
        };
        cards = cards.child(
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
                                .child(ui::icon(ic, 28., p.text)),
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
                .child(action),
        );
    }
    (c.t(Tr::OnboardingAllowPermissions), c.t(Tr::OnboardingPermissionsDesc), cards)
}

fn usage(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let tile = |i: usize,
                name: SharedString,
                desc: SharedString,
                on: bool,
                recommended: bool,
                cx: &mut Context<Onboarding>| {
        let tile_id = ElementId::from(("mode-tile", i));
        div()
            .id(tile_id.clone())
            .flex_1()
            .relative()
            .px(px(20.))
            .py(px(16.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(16.))
            .cursor_pointer()
            .bg(if on { p.accent_soft } else { p.card })
            .opacity(if on { 1. } else { 0.7 })
            .border_1()
            .border_color(ui::transparent())
            .on_hover(ui::hover_tracker(tile_id.clone()))
            .child(ui::hover_fill_if(&tile_id, Some(16.), ui::hex_alpha(p.seg, 0.6), !on))
            .child(ui::ring_overlay(("mode-ring", i), on, 16., p.border, p.accent))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.mode = if i == 0 { RecordMode::Toggle } else { RecordMode::Hold };
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(name, 16., FontWeight::BOLD, p.text))
                    .children(recommended.then(|| recommend_tag(p, c.t(Tr::OnboardingRecommended)))),
            )
            .child(text(desc, 12., FontWeight::NORMAL, p.muted))
    };
    let toggle = w.mode == RecordMode::Toggle;
    let key =
        Shortcut::parse(&cfg.general.shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or_default();
    (
        c.t(Tr::OnboardingChooseMode),
        c.t(Tr::OnboardingChooseModeDesc),
        div().w_full().flex().flex_col().gap(px(14.)).child(
            ui::card(p)
                .p(px(20.))
                .gap(px(18.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .gap(px(12.))
                        .child(tile(0, c.t(Tr::GeneralToggle), c.t(Tr::OnboardingToggleDesc), toggle, true, cx))
                        .child(tile(1, c.t(Tr::GeneralHold), c.t(Tr::OnboardingHoldDesc), !toggle, false, cx)),
                )
                .child(crate::onboarding_demo::usage_demo(p, c.lang, w.mode, key)),
        ),
    )
}

fn shortcut(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let current =
        Shortcut::parse(&cfg.general.shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or_default();
    let mut keys = div().flex().gap(px(10.));
    for (i, id) in ["RightOption", "RightCommand", "Fn"].into_iter().enumerate() {
        let on = !w.capturing && id == cfg.general.shortcut;
        let label = Shortcut::parse(id).map(|s| s.label(c.lang)).unwrap_or_default();
        let st = w.st.clone();
        keys = keys.child(key_tile(
            i,
            p,
            label.into(),
            on,
            cx.listener(move |this, _, _, cx| {
                if this.capturing {
                    this.st.hotkeys.cancel_capture();
                }
                let _ = st.cfg.update(|c| c.general.shortcut = id.to_string());
                st.hotkeys.set_shortcut(id);
                cx.notify();
            }),
        ));
    }
    let (hero, caption) = if w.capturing {
        (
            keycap_waiting(p, c.t(Tr::GeneralWaitingForInput)).into_any_element(),
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(text(c.t(Tr::OnboardingPressAnyKey), 14., FontWeight::MEDIUM, p.text))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .h(px(24.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .rounded(px(7.))
                                .bg(p.card)
                                .border_1()
                                .border_color(p.border_strong)
                                .child(text("Esc", 12., FontWeight::BOLD, p.text).line_height(relative(1.))),
                        )
                        .child(text(c.t(Tr::OnboardingToCancel), 13., FontWeight::NORMAL, p.muted)),
                ),
        )
    } else {
        (
            {
                let hero_id = ElementId::from("shortcut-hero");
                div()
                    .id(hero_id.clone())
                    .relative()
                    .cursor_pointer()
                    .on_hover(ui::hover_tracker(hero_id.clone()))
                    .on_click(cx.listener(|this, _, _, _| this.st.hotkeys.begin_capture()))
                    .child(keycap(p, &current, true))
                    .child(ui::hover_fill(&hero_id, Some(20.), ui::hex_alpha(p.seg, 0.6)))
                    .into_any_element()
            },
            div().child(text(c.t(Tr::OnboardingClickToRecord), 13., FontWeight::NORMAL, p.muted)),
        )
    };
    (
        c.t(Tr::OnboardingChooseShortcut),
        c.t(Tr::OnboardingChooseShortcutDesc),
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(ui::card(p).bg(p.sub).py(px(26.)).items_center().gap(px(14.)).child(hero).child(caption))
            .child(
                ui::card(p)
                    .px(px(22.))
                    .py(px(18.))
                    .items_center()
                    .gap(px(14.))
                    .child(text(c.t(Tr::OnboardingQuickSetup), 13., FontWeight::BOLD, p.text))
                    .child(keys),
            ),
    )
}

fn engine(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let opt = |i: usize,
               title: SharedString,
               recommended: bool,
               points: Vec<(Ic, SharedString)>,
               on: bool,
               cx: &mut Context<Onboarding>| {
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
                    .children(recommended.then(|| recommend_tag(p, c.t(Tr::OnboardingRecommended)))),
            );
        for (ic, s) in points {
            card = card.child(
                div().flex().items_center().gap(px(10.)).child(ui::icon(ic, 16., p.accent_text)).child(text(
                    s,
                    13.,
                    FontWeight::NORMAL,
                    p.text,
                )),
            );
        }
        card
    };
    (
        c.t(Tr::OnboardingRunLocally),
        c.t(Tr::OnboardingChooseEngineDesc),
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(opt(
                0,
                c.t(Tr::ModelsLocal),
                true,
                vec![
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
                ],
                w.local,
                cx,
            ))
            .child(opt(
                1,
                c.t(Tr::ModelsProvider),
                false,
                vec![
                    (Ic::Globe, c.t(Tr::OnboardingProviderCardProviders)),
                    (Ic::Lock, c.t(Tr::OnboardingProviderCardNeedsNetwork)),
                    (Ic::Cpu, c.t(Tr::OnboardingCardLight)),
                ],
                !w.local,
                cx,
            )),
    )
}

fn prepare_local(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let snap: Snapshot = w.st.models.subscribe().borrow().clone();
    let ids = [cfg.asr.local_model.clone(), cfg.polish.engine.local_model.clone()];
    let current = TIERS.iter().position(|t| t.asr == cfg.asr.local_model && t.polish == cfg.polish.engine.local_model);
    let phase = w.prepare_phase();
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
    let col = div().w_full().flex().flex_col().gap(px(16.)).child(tiers);
    let mut rows = ui::card(p).p(px(22.)).gap(px(16.));
    for (role, id) in [c.t(Tr::ModelsDictation), c.t(Tr::ModelsPolishShort)].into_iter().zip(ids) {
        let name = w.st.models.spec(&id).map(|s| s.name.clone()).unwrap_or_else(|| id.clone());
        let name = format!("{role} {name}");
        let state = snap.get(&id).cloned().unwrap_or(ModelState::NotInstalled);
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
        rows = rows.child(
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
                .child(ui::progress_bar(p, frac)),
        );
    }
    if phase != PreparePhase::Ready {
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
        rows = rows.child(
            div()
                .w_full()
                .pt(px(16.))
                .border_t_1()
                .border_color(p.border)
                .flex()
                .items_center()
                .justify_between()
                .child(text(note, 13., FontWeight::NORMAL, p.muted))
                .child(action),
        );
    }
    (c.t(Tr::OnboardingPrepareLocal), c.t(Tr::OnboardingPrepareDesc), col.child(rows))
}

fn prepare_provider(w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let kind = w.provider;
    let tiles = {
        let mut row = div().w_full().flex().gap(px(10.));
        for (k, name) in dito_core::provider::configured_asr::AsrProfiles::NAMES.iter().enumerate() {
            let on = k == kind;
            let card_id = ElementId::from(("wizard-prov", k));
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
                    .child(ui::ring_overlay(("wizard-ring", k), on, 14., p.border_strong, p.accent))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.apply_provider(cx);
                        this.provider = k;
                        let mut p = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                        p.selected = k;
                        this.endpoint.update(cx, |s, cx| {
                            s.set_value(p.active()["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
                        });
                        this.deployment.update(cx, |s, cx| {
                            s.set_value(p.active()["connection"]["model"].as_str().unwrap_or(""), window, cx)
                        });
                        this.key.update(cx, |s, cx| {
                            s.set_value(this.st.secrets.get(&p.account()).unwrap_or_default(), window, cx)
                        });
                        this.invalidate_test();
                        cx.notify();
                    }))
                    .child(ui::choice_label(&card_id, p, (*name).into(), 13., on)),
            );
        }
        row
    };
    let (l1, l2) = (c.t(Tr::ModelsBaseUrl), c.t(Tr::ModelsModelId));
    let mut form =
        ui::card(p).p(px(22.)).gap(px(16.)).child(tiles).child(ui::field(p, l1, ui::text_input(p, &w.endpoint, None)));
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
    let test = match &w.test {
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
    };
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
            .child(test),
    );
    if let TestState::Failed(message) = &w.test {
        form = form.child(text(ui::error_line(c.lang, message), 12., FontWeight::NORMAL, p.text));
    }
    (c.t(Tr::OnboardingConnectProvider), "".into(), form)
}

fn try_step(w: &Onboarding, c: &Ctx, _cx: &mut Context<Onboarding>) -> (SharedString, SharedString, Div) {
    let p = &c.p;
    let cfg = w.st.cfg.get();
    let key =
        Shortcut::parse(&cfg.general.shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or_default();
    let hold = w.mode == RecordMode::Hold;
    let steps = if hold {
        [dito_core::i18n::fmt::hold_key(c.lang, &key), c.t(Tr::DemoSpeak).to_string(), c.t(Tr::DemoRelease).to_string()]
    } else {
        [
            dito_core::i18n::fmt::press_key(c.lang, &key),
            c.t(Tr::DemoSpeak).to_string(),
            c.t(Tr::DemoPressAgain).to_string(),
        ]
    };
    let mut row = div().w_full().flex().gap(px(12.));
    for (i, s) in steps.into_iter().enumerate() {
        let done = w.seen[i];
        row = row.child(
            ui::card(p)
                .flex_1()
                .px(px(16.))
                .py(px(14.))
                .gap(px(8.))
                .child(
                    div()
                        .size(px(26.))
                        .rounded_full()
                        .bg(if done { p.success } else { p.seg })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon(Ic::Check, 14., if done { ui::white() } else { p.muted })),
                )
                .child(text(s, 13., FontWeight::MEDIUM, p.text)),
        );
    }
    let caret = || div().mt(px(5.)).w(px(2.)).h(px(24.)).flex_none().rounded(px(1.)).bg(p.accent);
    // An empty field shows the caret before the placeholder, as a real text field does.
    let input = div()
        .w_full()
        .flex_1()
        .px(px(18.))
        .py(px(14.))
        .rounded(px(12.))
        .bg(p.input_bg)
        .border_1()
        .border_color(p.accent)
        .flex()
        .flex_wrap()
        .items_start();
    let input = match &w.tried_text {
        Some(s) => input
            .child(text(s.clone(), 16., FontWeight::NORMAL, p.text).line_height(relative(1.7)))
            .child(caret().ml(px(2.))),
        None => input
            .child(caret().mr(px(2.)))
            .child(text(c.t(Tr::OnboardingTrySpeaking), 16., FontWeight::NORMAL, p.muted).line_height(relative(1.7))),
    };
    let field = div()
        .w_full()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(text(c.t(Tr::OnboardingTextAppearsHere), 11., FontWeight::MEDIUM, p.muted))
        .child(input);
    (
        c.t(Tr::OnboardingTryTitle),
        if hold { c.t(Tr::OnboardingTryHoldDesc) } else { c.t(Tr::OnboardingTryToggleDesc) },
        div().w_full().h_full().flex().flex_col().gap(px(16.)).child(row).child(field),
    )
}

// ---- welcome ----

/// The welcome column: logo, tagline (14 pt under it), then 56 pt down the button.
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
pub(super) fn welcome(_w: &Onboarding, c: &Ctx, cx: &mut Context<Onboarding>) -> Div {
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

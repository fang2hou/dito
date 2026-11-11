use std::sync::Arc;
use std::time::Duration;

use dito_core::config::Source;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_platform::Status;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::permissions::read_perms;
use crate::assets::Ic;
use crate::services;
use crate::settings::{self, Ctx, Page, role};
use crate::state::AppState;
use crate::ui::{self, BtnKind, TestState, cbk, text};

/// Status cards are read-only: starting, restarting, retrying and enabling live in Models or
/// General settings. This page reports engine state, provider facts, permissions — nothing else.
pub struct StatusPage {
    pub(super) st: Arc<AppState>,
    pub(super) checks: [TestState; 2],
    pub(super) checked_at: [Option<u64>; 2],
    pub(super) perms: [Status; 2],
    pub(super) motion: ui::MotionScope,
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl StatusPage {
    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for StatusPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl StatusPage {
    pub fn new(st: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let perms = read_perms();
                if this
                    .update(cx, |t, cx| {
                        t.perms = perms;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // Probe provider connections once so cards can show latency without a manual recheck.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(350)).await;
            let _ = this.update(cx, |t, cx| t.probe_providers(cx));
        })
        .detach();
        Self {
            motion: ui::MotionScope::new(),
            st,
            checks: [TestState::Idle, TestState::Idle],
            checked_at: [None, None],
            perms: read_perms(),
        }
    }

    fn probe_providers(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if cfg.asr.source == Source::Provider {
            self.run_check(Role::Asr, cx);
        }
        if cfg.polish.enabled && cfg.polish.engine.source == Source::Provider {
            self.run_check(Role::Polish, cx);
        }
    }

    fn run_check(&mut self, role: Role, cx: &mut Context<Self>) {
        let idx = role::index(role);
        if self.checks[idx] == TestState::Running {
            return;
        }
        self.checks[idx] = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = services::test_connection(st, role).await;
            let _ = this.update(cx, |t, cx| {
                let idx = role::index(role);
                t.checked_at[idx] = Some(now_epoch());
                t.checks[idx] = outcome.into();
                cx.notify();
            });
        })
        .detach();
    }

    fn check_all(&mut self, cx: &mut Context<Self>) {
        self.perms = read_perms();
        self.probe_providers(cx);
        cx.notify();
    }

    fn view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        settings::page(
            p,
            "status",
            c.t(Tr::NavStatus),
            Some(self.actions(&c, cx)),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(28.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(text(c.t(Tr::NavModels), 13., FontWeight::BOLD, p.muted))
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap(px(12.))
                                .child(self.engine_card(&c, Role::Asr))
                                .child(self.engine_card(&c, Role::Polish)),
                        ),
                )
                .child(self.permissions_card(&c, cx)),
        )
    }

    fn actions(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let p = c.p;
        div()
            .flex()
            .gap(px(10.))
            .child(ui::button(
                "manage-models",
                &p,
                &c.t(Tr::StatusManageModels),
                BtnKind::Secondary,
                Some(Ic::Cpu),
                std::rc::Rc::new(|_, cx| {
                    let st = settings::global_state(cx);
                    settings::open(st, Page::Models, cx);
                }),
            ))
            .child(ui::button(
                "check-all",
                &p,
                &c.t(Tr::StatusCheckAll),
                BtnKind::Secondary,
                Some(Ic::Refresh),
                cbk(cx, |this: &mut Self, _, cx| this.check_all(cx)),
            ))
            .into_any_element()
    }
}

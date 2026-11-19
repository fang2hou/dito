use std::sync::Arc;

use dito_core::config::Source;
use dito_core::i18n::{Tr, fmt};
use dito_core::secrets::Role;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::log::LogText;
use super::steps::{Dot, Step};
use super::usage::{SAMPLE_EVERY, UsageMeter};
use crate::assets::Ic;
use crate::services;
use crate::settings::{self, Ctx, role};
use crate::state::AppState;
use crate::ui::{self, BtnKind, TestState, cbk, text};

const LOG_REDRAW: std::time::Duration = std::time::Duration::from_millis(150);

/// `design:settings/status`: can the user dictate now, and if not, what to fix.
pub struct StatusPage {
    pub(super) st: Arc<AppState>,
    pub(super) checks: [TestState; 2],
    pub(super) microphone_allowed: bool,
    pub(super) accessibility_allowed: bool,
    pub(super) system_audio: Status,
    pub(super) usage: UsageMeter,
    /// Lives only while the page is on screen: dropping it stops the permission and usage readings.
    refresh: Option<Task<()>>,
    /// Redraws the log when lines arrive; held only while the page is on screen.
    log_watch: Option<Task<()>>,
    pub(super) log_scroll: ScrollHandle,
    /// Built by the first render that needs it and dropped when the page leaves the screen.
    pub(super) log_text: Option<LogText>,
    motion: ui::MotionScope,
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
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

impl StatusPage {
    pub fn new(st: Arc<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        Self {
            motion: ui::MotionScope::new(),
            st,
            checks: [TestState::Idle, TestState::Idle],
            microphone_allowed: true,
            accessibility_allowed: true,
            // The first reading lands before the page is shown; until then, report no problem.
            system_audio: Status::Granted,
            usage: UsageMeter::default(),
            refresh: None,
            log_watch: None,
            log_scroll: ScrollHandle::new(),
            log_text: None,
        }
    }

    /// Nothing is read or measured while the page is off screen.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            self.refresh = None;
            self.log_watch = None;
            self.log_text = None;
            self.usage.stop();
            return;
        }
        self.watch_log(cx);
        self.read();
        if self.checks.iter().all(|check| *check == TestState::Idle) {
            self.probe_providers(cx);
        }
        self.refresh = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SAMPLE_EVERY).await;
                let alive = this.update(cx, |page, cx| {
                    page.read();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// A burst of lines redraws once.
    fn watch_log(&mut self, cx: &mut Context<Self>) {
        let mut changes = self.st.log.subscribe();
        self.log_scroll.scroll_to_bottom();
        self.log_watch = Some(cx.spawn(async move |this, cx| {
            while changes.changed().await.is_ok() {
                cx.background_executor().timer(LOG_REDRAW).await;
                changes.borrow_and_update();
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }

    fn read(&mut self) {
        // Without a real capture the recorder is synthetic and needs no permission.
        self.microphone_allowed =
            self.st.capture.is_none() || permissions::status(Permission::Microphone) == Status::Granted;
        self.accessibility_allowed = permissions::status(Permission::Accessibility) == Status::Granted;
        self.system_audio = permissions::status(Permission::SystemAudio);
        self.usage.sample(&self.st);
    }

    fn probe_providers(&mut self, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        if cfg.asr.source == Source::Provider {
            self.run_check(Role::Asr, cx);
        }
        if cfg.polish.enabled && cfg.text_model.source == Source::Provider {
            self.run_check(Role::Text, cx);
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
                t.checks[role::index(role)] = outcome.into();
                cx.notify();
            });
        })
        .detach();
    }

    fn check_all(&mut self, cx: &mut Context<Self>) {
        self.read();
        self.probe_providers(cx);
        cx.notify();
    }

    fn view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        self.refresh_log_text(&p);
        let steps = self.steps(&c);
        let ready = steps.iter().all(|step| !step.blocks_dictation || step.dot != Dot::Attention);
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
                .child(self.verdict(&c, ready))
                .child(self.flow(&c, cx, steps))
                .children(self.usage.table(&c))
                .child(self.log_group(&c, cx)),
        )
    }

    fn actions(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        ui::button(
            "check-all",
            &c.p,
            &c.t(Tr::StatusCheckAll),
            BtnKind::Secondary,
            Some(Ic::Refresh),
            cbk(cx, |this: &mut Self, _, cx| this.check_all(cx)),
        )
        .into_any_element()
    }

    fn verdict(&self, c: &Ctx, ready: bool) -> Div {
        let p = c.p;
        let (color, title, summary) = if ready {
            let summary = fmt::status_ready_summary(c.lang, &self.shortcut_label(c));
            (p.success, c.t(Tr::StatusReady), SharedString::from(summary))
        } else {
            (p.danger, c.t(Tr::StatusNeedsAttention), c.t(Tr::StatusFixRedItems))
        };
        ui::card(&p).child(
            div()
                .w_full()
                .px(px(24.))
                .py(px(22.))
                .flex()
                .items_start()
                .gap(px(14.))
                .child(
                    div()
                        .h(px(30.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .child(div().size(px(14.)).rounded_full().bg(color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(text(title, 20., FontWeight::BOLD, p.text).line_height(px(30.)))
                        .child(
                            text(summary, 14., FontWeight::NORMAL, p.muted).whitespace_normal().line_height(px(22.)),
                        ),
                ),
        )
    }

    fn flow(&self, c: &Ctx, cx: &mut Context<Self>, steps: [Step; 6]) -> Div {
        let mut card = ui::card(&c.p);
        for (index, step) in steps.into_iter().enumerate() {
            if index > 0 {
                card = card.child(div().w_full().h(px(1.)).bg(c.p.border));
            }
            card = card.child(self.step_row(c, cx, step));
        }
        card
    }
}

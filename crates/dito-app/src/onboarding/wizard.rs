use std::sync::Arc;
use std::time::Instant;

use dito_core::config::{RecordMode, Source};
use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::*;

use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::ui::{self, InputEvent, InputState, Motion, TestState};

use super::step::{Step, can_continue};
use super::transition::Slide;
use super::window::WizardWindow;

mod connection;
mod prepare;
#[cfg(feature = "screenshots")]
mod probe;
mod render;
mod watchers;

pub(super) use prepare::PreparePhase;

pub(super) const PERMS: [Permission; 2] = [Permission::Microphone, Permission::Accessibility];

pub struct Onboarding {
    pub(super) st: Arc<AppState>,
    pub(super) step: Step,
    pub(super) mode: RecordMode,
    pub(super) local: bool,
    /// Id of the cloud speech configuration the Prepare step edits.
    pub(super) cloud_asr_id: String,
    pub(super) editor: Option<Entity<crate::settings::asr_editor::AsrEditor>>,
    pub(super) endpoint: Entity<InputState>,
    pub(super) deployment: Entity<InputState>,
    pub(super) key: Entity<InputState>,
    pub(super) test: TestState,
    /// Generation of the connection-test request whose result may still land.
    pub(super) reqs: ui::Requests,
    pub(super) perms: [Status; 2],
    pub(super) capturing: bool,
    pub(super) seen: [bool; 3],
    pub(super) started_at: i64,
    pub(super) tried_text: Option<String>,
    pub(super) tried_ok: bool,
    pub(super) prepare_started: bool,
    /// When はじめる was pressed (and whether Reduce Motion is on), while the welcome page opens into the first step.
    pub(super) slide: Option<Slide>,
    /// The current step's motion scope: its controls' transient state, renewed on every step change.
    pub(super) motion: ui::MotionScope,
    /// The leaving step's scope, used only while a slide runs. The owner is retained for the whole
    /// wizard so the page sliding out keeps its state; it is reset at the same lifecycle points the
    /// old wizard-wide reset ran (step change, slide end).
    pub(super) leaving_motion: ui::MotionScope,
}

impl Onboarding {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let configurations = cfg.asr.cloud_asr.clone().unwrap_or_default();
        let [endpoint, deployment, key] = Self::connection_inputs(&st, &configurations, window, cx);
        for input in [&endpoint, &deployment, &key] {
            cx.subscribe_in(input, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.invalidate_test();
                    cx.notify();
                }
            })
            .detach();
        }
        Self::watch_capture(&st, cx);
        Self::watch_models(&st, cx);
        Self::watch_try_progress(&st, cx);
        Self::poll_permissions(cx);

        Self {
            mode: cfg.general.record_mode,
            local: cfg.asr.source == Source::Local,
            cloud_asr_id: configurations.selected_id().to_owned(),
            editor: None,
            endpoint,
            deployment,
            key,
            test: TestState::Idle,
            reqs: ui::Requests::default(),
            perms: PERMS.map(permissions::status),
            capturing: false,
            seen: [false; 3],
            started_at: crate::timefmt::now(),
            tried_text: None,
            tried_ok: false,
            prepare_started: false,
            slide: None,
            motion: ui::MotionScope::new(),
            leaving_motion: ui::MotionScope::new(),
            st,
            step: Step::Welcome,
        }
    }

    fn connection_inputs(
        st: &AppState,
        configurations: &dito_core::provider::configured_asr::CloudAsrConfigurations,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> [Entity<InputState>; 3] {
        let lang = st.cfg.get().general.lang();
        let mut input = |placeholder: &str, value: &str, masked: bool| {
            let (placeholder, value) = (placeholder.to_string(), value.to_string());
            let state = cx.new(|cx| {
                let s = InputState::new(window, cx).placeholder(placeholder).default_value(value);
                if masked { s.masked(true) } else { s }
            });
            ui::notify_on_focus(&state, window, cx);
            state
        };
        let connection = |field: &str| configurations.active()["connection"][field].as_str().unwrap_or("").to_string();
        [
            input("https://…", &connection("base_url"), false),
            input(lang.tr(Tr::ModelsModelPlaceholder), &connection("model"), false),
            input("••••••••", &st.secrets.get(&configurations.account()).unwrap_or_default(), true),
        ]
    }

    /// Forget the hover and tween state of every control in the wizard. Controls here share ids across steps (the
    /// footer's buttons), so state remembered from one page would otherwise carry over to the next, and a pointer
    /// that is over a button when the page changes would never light it again.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
        self.leaving_motion.reset();
    }

    pub(super) fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        if self.capturing {
            self.st.hotkeys.cancel_capture();
        }
        if step != self.step {
            self.reset_motion();
            self.supersede_pending_test();
            self.slide = Some(Slide {
                from: self.step,
                forward: step.number() > self.step.number(),
                motion: Motion::system(),
                at: Instant::now(),
            });
        }
        self.step = step;
        if step == Step::Try {
            self.started_at = crate::timefmt::now();
            self.seen = [false; 3];
            self.tried_text = None;
            self.tried_ok = false;
        }
        if step == Step::Prepare {
            // Nothing is downloaded until the user asks; a download that is already running (they stepped
            // back and forth) just keeps showing as running.
            self.prepare_started = self.local_ids().iter().any(|id| self.st.models.state(id).is_active());
        }
        cx.notify();
    }

    pub(super) fn next(&mut self, cx: &mut Context<Self>) {
        if self.slide.is_some() {
            return;
        }
        let target = self.step.next();
        match self.step {
            Step::Welcome | Step::Permissions | Step::Shortcut => {}
            Step::Usage => {
                let mode = self.mode;
                services::save_config(&self.st, |c| c.general.record_mode = mode);
            }
            Step::Engine => {
                let src = if self.local { Source::Local } else { Source::Provider };
                services::save_config(&self.st, |c| {
                    c.asr.source = src;
                    if self.local {
                        c.polish.engine.source = Source::Local;
                    }
                });
            }
            Step::Prepare => {
                if !self.local {
                    self.apply_cloud_asr(cx);
                }
            }
            Step::Try => {
                services::save_config(&self.st, |c| c.onboarded = true);
                if !self.st.opts.mock
                    && let Err(e) =
                        dito_platform::system::set_launch_at_login(self.st.cfg.get().general.launch_at_login)
                {
                    tracing::warn!("login item: {e}");
                }
                let st = self.st.clone();
                cx.defer(move |cx| {
                    if let Some(h) = cx.try_global::<WizardWindow>().map(|h| h.0) {
                        let _ = h.update(cx, |_, window, _| window.remove_window());
                        cx.remove_global::<WizardWindow>();
                    }
                    settings::open(st, Page::General, cx);
                });
            }
        }
        if let Some(target) = target {
            self.go(target, cx);
        }
    }

    pub(super) fn back(&mut self, cx: &mut Context<Self>) {
        if self.slide.is_some() {
            return;
        }
        if let Some(prev) = self.step.prev() {
            self.go(prev, cx);
        }
    }

    /// 続ける / 完了 stays disabled until the step's work is done.
    pub(super) fn can_next(&self, _cx: &App) -> bool {
        let cfg = self.st.cfg.get();
        let models_installed =
            [&cfg.asr.local_model, &cfg.polish.engine.local_model].iter().all(|id| self.st.models.is_installed(id));
        can_continue(self.step, self.local, models_installed, matches!(self.test, TestState::Ok(_)), self.tried_ok)
    }
}

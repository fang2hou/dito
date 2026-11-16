use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use dito_core::config::{RecordMode, Source};
use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::*;

use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::ui::{self, InputEvent, InputState, Motion, TestState, TextareaState};

use super::step::{Step, can_continue};
use super::transition::Slide;
use super::window::WizardWindow;

mod connection;
mod prepare;
#[cfg(feature = "screenshots")]
mod probe;
mod render;
mod watchers;

use connection::{connection_fields, starting_cloud_asr};
pub(super) use prepare::PreparePhase;

pub(super) const PERMS: [Permission; 3] = [Permission::Microphone, Permission::Accessibility, Permission::SystemAudio];

pub struct Onboarding {
    pub(super) st: Arc<AppState>,
    pub(super) step: Step,
    pub(super) mode: RecordMode,
    pub(super) local: bool,
    pub(super) cloud_asr_id: String,
    pub(super) editor: Option<Entity<crate::settings::asr_editor::AsrEditor>>,
    pub(super) endpoint: Entity<InputState>,
    pub(super) deployment: Entity<InputState>,
    pub(super) key: Entity<InputState>,
    pub(super) test: TestState,
    /// Generation of the connection-test request whose result may still land.
    pub(super) reqs: ui::Requests,
    pub(super) perms: [Status; 3],
    pub(super) capturing: bool,
    pub(super) seen: [bool; 3],
    pub(super) try_input: Entity<TextareaState>,
    pub(super) try_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) focus_try: bool,
    pub(super) tried_ok: bool,
    pub(super) prepare_started: bool,
    /// Also covers the welcome page opening into the first step.
    pub(super) slide: Option<Slide>,
    /// Its controls' transient state is renewed on every step change.
    pub(super) motion: ui::MotionScope,
    /// The leaving step's scope, used only while a slide runs. The owner is retained for the whole wizard so the page
    /// sliding out keeps its state; it resets on a step change and when the slide ends.
    pub(super) leaving_motion: ui::MotionScope,
}

impl Onboarding {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let configurations = cfg.asr.cloud_asr.clone().unwrap_or_default();
        let [endpoint, deployment, key] = Self::connection_inputs(&st, &configurations, window, cx);
        let try_input =
            cx.new(|cx| TextareaState::new(window, cx).placeholder(cfg.general.lang().tr(Tr::OnboardingTrySpeaking)));
        let focus = try_input.read(cx).focus_handle(cx).clone();
        cx.on_focus_in(&focus, window, |_, _, cx| cx.notify()).detach();
        cx.on_focus_out(&focus, window, |_, _, _, cx| cx.notify()).detach();
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
            cloud_asr_id: starting_cloud_asr(&configurations).map(|c| c.id.clone()).unwrap_or_default(),
            editor: None,
            endpoint,
            deployment,
            key,
            test: TestState::Idle,
            reqs: ui::Requests::default(),
            perms: PERMS.map(permissions::status),
            capturing: false,
            seen: [false; 3],
            try_input,
            try_bounds: Rc::default(),
            focus_try: false,
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
        let starting = starting_cloud_asr(configurations);
        let (base_url, model) = connection_fields(starting);
        let key = starting.and_then(|c| st.secrets.get(&c.account())).unwrap_or_default();
        [
            input("https://…", &base_url, false),
            input(lang.tr(Tr::ModelsModelPlaceholder), &model, false),
            input("••••••••", &key, true),
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
            self.seen = [false; 3];
            self.tried_ok = false;
            self.focus_try = true;
        }
        if step == Step::Prepare {
            // Nothing is downloaded until the user asks; a download that is already running (they stepped back and
            // forth) just keeps showing as running.
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
                        c.text_model.source = Source::Local;
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

    pub(super) fn can_next(&self, _cx: &App) -> bool {
        let cfg = self.st.cfg.get();
        let models_installed =
            [&cfg.asr.local_model, &cfg.text_model.local_model].iter().all(|id| self.st.models.is_installed(id));
        can_continue(self.step, self.local, models_installed, matches!(self.test, TestState::Ok(_)), self.tried_ok)
    }
}

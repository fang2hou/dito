//! First-run wizard. It only sets initial values; every setting stays editable in exactly one place afterwards.
//!
//! One `Onboarding` entity coordinates the whole wizard: it owns the state, the step transitions
//! ([`transition`]) and the window ([`window`]); each step's content lives in [`steps`], and the
//! pieces reused across steps in [`components`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{RecordMode, Source};
use dito_core::controller::HudState;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_core::store::HistoryFilter;
use dito_local::ModelState;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::hotkeys::CaptureState;
use crate::services;
use crate::settings::{self, Page};
use crate::state::AppState;
use crate::theme;
use crate::ui::{self, TestState, text};

use transition::Slide;

mod components;
mod steps;
mod transition;
mod window;

#[cfg(feature = "screenshots")]
pub use window::open_with_view;
pub use window::{WizardWindow, open, restart};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Welcome,
    Permissions,
    Usage,
    Shortcut,
    Engine,
    Prepare,
    Try,
}

impl Step {
    fn number(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Permissions => 1,
            Self::Usage => 2,
            Self::Shortcut => 3,
            Self::Engine => 4,
            Self::Prepare => 5,
            Self::Try => 6,
        }
    }
}

/// Where the local-model preparation stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PreparePhase {
    /// Nothing is running; 準備を始める is offered.
    Idle,
    Downloading,
    /// A model failed to download; the button offers 再試行.
    Failed,
    Ready,
}

const PERMS: [Permission; 2] = [Permission::Microphone, Permission::Accessibility];

/// Counts connection-test requests so that stale completions are dropped: beginning a new request, or
/// entering, leaving or restarting a step, supersedes every earlier generation, and only the newest one
/// may still write its result.
#[derive(Default)]
struct Requests {
    generation: u64,
}

impl Requests {
    /// Begin a new request and return its generation.
    fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Invalidate every in-flight request.
    fn bump(&mut self) {
        self.generation += 1;
    }

    /// Whether the request that got `generation` is still the newest one.
    fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }
}

pub struct Onboarding {
    st: Arc<AppState>,
    step: Step,
    mode: RecordMode,
    local: bool,
    provider: usize,
    editor: Option<Entity<crate::settings::asr_editor::AsrEditor>>,
    endpoint: Entity<InputState>,
    deployment: Entity<InputState>,
    key: Entity<InputState>,
    test: TestState,
    /// Generation of the connection-test request whose result may still land.
    reqs: Requests,
    perms: [Status; 2],
    capturing: bool,
    seen: [bool; 3],
    started_at: i64,
    tried_text: Option<String>,
    tried_ok: bool,
    prepare_started: bool,
    /// When はじめる was pressed (and whether Reduce Motion is on), while the welcome page opens into the first step.
    slide: Option<Slide>,
    /// The current step's motion scope: its controls' transient state, renewed on every step change.
    motion: ui::MotionScope,
    /// The leaving step's scope, used only while a slide runs. The owner is retained for the whole
    /// wizard so the page sliding out keeps its state; it is reset at the same lifecycle points the
    /// old wizard-wide reset ran (step change, slide end).
    leaving_motion: ui::MotionScope,
}

impl Onboarding {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::settings::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let mk = |window: &mut Window, cx: &mut Context<Self>, ph: &str, val: &str, masked: bool| {
            let (ph, val) = (ph.to_string(), val.to_string());
            let state = cx.new(|cx| {
                let s = InputState::new(window, cx).placeholder(ph).default_value(val);
                if masked { s.masked(true) } else { s }
            });
            ui::notify_on_focus(&state, window, cx);
            state
        };
        let profiles = cfg.asr.cloud_asr.clone().unwrap_or_default();
        let endpoint =
            mk(window, cx, "https://…", profiles.active()["connection"]["base_url"].as_str().unwrap_or(""), false);
        let deployment = mk(
            window,
            cx,
            cfg.general.lang().tr(Tr::ModelsModelPlaceholder),
            profiles.active()["connection"]["model"].as_str().unwrap_or(""),
            false,
        );
        let key = mk(window, cx, "••••••••", &st.secrets.get(&profiles.account()).unwrap_or_default(), true);
        for ent in [&endpoint, &deployment, &key] {
            cx.subscribe_in(ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.invalidate_test();
                    cx.notify();
                }
            })
            .detach();
        }

        let mut cap = st.hotkeys.capture.subscribe();
        cx.spawn(async move |this, cx| {
            while cap.changed().await.is_ok() {
                let w = *cap.borrow_and_update() == CaptureState::Waiting;
                if this
                    .update(cx, |t, cx| {
                        t.capturing = w;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let mut models = st.models.subscribe();
        cx.spawn(async move |this, cx| {
            while models.changed().await.is_ok() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        // Track the three "try it" milestones from the HUD, and poll permissions.
        let mut hud = st.dictation.hud();
        cx.spawn(async move |this, cx| {
            while hud.changed().await.is_ok() {
                let s = hud.borrow_and_update().clone();
                if this
                    .update(cx, |t, cx| {
                        if t.step == Step::Try {
                            // Hold: releasing the key (recognition starts) completes both "speak" and "release".
                            let hold = t.mode == RecordMode::Hold;
                            match s {
                                HudState::Listening { .. } => t.seen[0] = true,
                                HudState::Recognizing => {
                                    t.seen[1] = true;
                                    t.seen[2] |= hold;
                                }
                                HudState::Inserted | HudState::Copied => {
                                    t.seen[2] = true;
                                    t.tried_ok = true;
                                }
                                _ => {}
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1200)).await;
                let perms = PERMS.map(permissions::status);
                if this
                    .update(cx, |t, cx| {
                        t.perms = perms;
                        if t.step == Step::Try && t.tried_text.is_none() {
                            t.tried_text =
                                t.st.store
                                    .list_history(&HistoryFilter {
                                        since_ts: Some(t.started_at),
                                        limit: Some(1),
                                        ..Default::default()
                                    })
                                    .ok()
                                    .and_then(|v| v.first().map(|e| e.final_text().to_string()));
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Self {
            mode: cfg.general.record_mode,
            local: cfg.asr.source == Source::Local,
            provider: profiles.selected.min(2),
            editor: None,
            endpoint,
            deployment,
            key,
            test: TestState::Idle,
            reqs: Requests::default(),
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

    /// Forget the hover and tween state of every control in the wizard. Controls here share ids across steps (the
    /// footer's buttons), so state remembered from one page would otherwise carry over to the next, and a pointer
    /// that is over a button when the page changes would never light it again.
    fn reset_motion(&mut self) {
        self.motion.reset();
        self.leaving_motion.reset();
    }

    #[cfg(feature = "screenshots")]
    pub fn show_step(&mut self, n: usize, cx: &mut Context<Self>) {
        const ALL: [Step; 7] =
            [Step::Welcome, Step::Permissions, Step::Usage, Step::Shortcut, Step::Engine, Step::Prepare, Step::Try];
        self.step = ALL[n];
        self.local = n != 6 || self.local;
        self.supersede_pending_test();
        cx.notify();
    }

    #[cfg(feature = "screenshots")]
    pub fn set_mode(&mut self, mode: RecordMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    #[cfg(feature = "screenshots")]
    pub fn show_provider_form(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Prepare;
        self.local = false;
        self.supersede_pending_test();
        cx.notify();
    }

    /// The tested configuration changed (endpoint, key or provider edit): the recorded result no longer
    /// describes it, and any in-flight request must be superseded so its success cannot later overwrite
    /// the fresh idle state for the obsolete configuration.
    fn invalidate_test(&mut self) {
        self.reqs.bump();
        self.test = TestState::Idle;
    }

    /// A step change makes any in-flight connection test stale: entering, leaving or restarting a step
    /// supersedes the pending request, and a "connecting" state whose result is dropped would never clear.
    /// A result that already landed before the change stays.
    fn supersede_pending_test(&mut self) {
        self.reqs.bump();
        if matches!(self.test, TestState::Running) {
            self.test = TestState::Idle;
        }
    }

    fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        if self.capturing {
            self.st.hotkeys.cancel_capture();
        }
        if step != self.step {
            self.reset_motion();
            self.supersede_pending_test();
            self.slide = Some(Slide {
                from: self.step,
                forward: step.number() > self.step.number(),
                reduced: dito_platform::system::reduce_motion(),
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

    fn local_ids(&self) -> [String; 2] {
        let cfg = self.st.cfg.get();
        [cfg.asr.local_model.clone(), cfg.polish.engine.local_model.clone()]
    }

    fn prepare_phase(&self) -> PreparePhase {
        let states = self.local_ids().map(|id| self.st.models.state(&id));
        if states.iter().all(|s| *s == ModelState::Installed) {
            PreparePhase::Ready
        } else if states.iter().any(|s| s.is_active())
            || (self.prepare_started && !states.iter().any(|s| matches!(s, ModelState::Failed { .. })))
        {
            PreparePhase::Downloading
        } else if states.iter().any(|s| matches!(s, ModelState::Failed { .. })) {
            PreparePhase::Failed
        } else {
            PreparePhase::Idle
        }
    }

    /// 準備を始める: the only place a local model download begins.
    fn start_prepare(&mut self, cx: &mut Context<Self>) {
        if matches!(self.prepare_phase(), PreparePhase::Downloading | PreparePhase::Ready) {
            return;
        }
        self.prepare_started = true;
        for id in self.local_ids() {
            let _ = self.st.models.start_download(&id);
        }
        cx.notify();
    }

    fn models_bytes(&self, ids: &[&str]) -> u64 {
        ids.iter().filter_map(|id| self.st.models.spec(id)).flat_map(|s| s.files.iter().map(|f| f.size)).sum()
    }

    /// Choose the local ASR + polish pair. It only changes the choice: the download waits for 準備を始める, and the
    /// choice is locked while one runs.
    fn pick_tier(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.prepare_phase() == PreparePhase::Downloading {
            return;
        }
        let t = &steps::TIERS[i];
        let _ = self.st.cfg.update(|c| {
            c.asr.local_model = t.asr.to_string();
            c.polish.engine.local_model = t.polish.to_string();
        });
        cx.notify();
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        if self.slide.is_some() {
            return;
        }
        let cfg = self.st.cfg.get();
        match self.step {
            Step::Welcome => self.go(Step::Permissions, cx),
            Step::Permissions => self.go(Step::Usage, cx),
            Step::Usage => {
                let mode = self.mode;
                let _ = self.st.cfg.update(|c| c.general.record_mode = mode);
                self.go(Step::Shortcut, cx);
            }
            Step::Shortcut => self.go(Step::Engine, cx),
            Step::Engine => {
                let src = if self.local { Source::Local } else { Source::Provider };
                let _ = self.st.cfg.update(|c| {
                    c.asr.source = src;
                    if self.local {
                        c.polish.engine.source = Source::Local;
                    }
                });
                self.go(Step::Prepare, cx);
            }
            Step::Prepare => {
                if !self.local {
                    self.apply_provider(cx);
                }
                self.go(Step::Try, cx);
            }
            Step::Try => {
                let _ = cfg;
                let _ = self.st.cfg.update(|c| c.onboarded = true);
                if !self.st.opts.mock
                    && let Err(e) =
                        dito_platform::system::set_launch_at_login(self.st.cfg.get().general.launch_at_login)
                {
                    tracing::warn!("login item: {e}");
                }
                let st = self.st.clone();
                // Close the wizard, open settings.
                cx.defer(move |cx| {
                    if let Some(h) = cx.try_global::<WizardWindow>().map(|h| h.0) {
                        let _ = h.update(cx, |_, window, _| window.remove_window());
                        cx.remove_global::<WizardWindow>();
                    }
                    settings::open(st, Page::General, cx);
                });
            }
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.slide.is_some() {
            return;
        }
        let prev = match self.step {
            Step::Welcome => return,
            Step::Permissions => Step::Welcome,
            Step::Usage => Step::Permissions,
            Step::Shortcut => Step::Usage,
            Step::Engine => Step::Shortcut,
            Step::Prepare => Step::Engine,
            Step::Try => Step::Prepare,
        };
        self.go(prev, cx);
    }

    fn open_connection_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_provider(cx);
        let editor = cx.new(|cx| crate::settings::asr_editor::AsrEditor::new(self.st.clone(), window, cx));
        cx.subscribe_in(&editor, window, |this, _, event: &crate::settings::asr_editor::Closed, window, cx| {
            if event.0 {
                let p = this.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
                this.provider = p.selected.min(2);
                this.endpoint.update(cx, |s, cx| {
                    s.set_value(p.active()["connection"]["base_url"].as_str().unwrap_or(""), window, cx)
                });
                this.deployment.update(cx, |s, cx| {
                    s.set_value(p.active()["connection"]["model"].as_str().unwrap_or(""), window, cx)
                });
                this.key
                    .update(cx, |s, cx| s.set_value(this.st.secrets.get(&p.account()).unwrap_or_default(), window, cx));
                this.invalidate_test();
            }
            this.editor = None;
            cx.notify();
        })
        .detach();
        self.editor = Some(editor);
        cx.notify();
    }

    fn apply_provider(&self, cx: &App) {
        let (selected, base, model) =
            (self.provider, self.endpoint.read(cx).value().to_string(), self.deployment.read(cx).value().to_string());
        let _ = self.st.cfg.update(|c| {
            let p = c.asr.cloud_asr.get_or_insert_with(dito_core::provider::configured_asr::AsrProfiles::default);
            p.selected = selected;
            if let Some(v) = p.profiles.get_mut(selected) {
                v["connection"]["base_url"] = base.into();
                v["connection"]["model"] = model.into();
            }
        });
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let key = self.key.read(cx).value().to_string();
        let _ = if key.is_empty() {
            self.st.secrets.delete(&profiles.account())
        } else {
            self.st.secrets.set(&profiles.account(), &key)
        };
    }

    fn run_test(&mut self, cx: &mut Context<Self>) {
        let req = self.reqs.begin();
        self.apply_provider(cx);
        let profiles = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        if dito_core::provider::configured_asr::validate(profiles.active()).is_err() {
            return;
        }
        self.test = TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let r = services::test_connection(st, Role::Asr).await;
            let _ = this.update(cx, |t, cx| {
                // A newer request or a step change has superseded this one; its result must not overwrite
                // the current step's request or state.
                if !t.reqs.is_current(req) {
                    return;
                }
                t.test = match r {
                    dito_core::provider::TestOutcome::Ok { latency_ms } => TestState::Ok(latency_ms),
                    dito_core::provider::TestOutcome::Failed { kind, message } => {
                        TestState::Failed(format!("{kind:?}: {message}"))
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// 続ける / 完了 stays disabled until the step's work is done.
    fn can_next(&self, _cx: &App) -> bool {
        let cfg = self.st.cfg.get();
        match self.step {
            Step::Prepare if self.local => {
                [&cfg.asr.local_model, &cfg.polish.engine.local_model].iter().all(|id| self.st.models.is_installed(id))
            }
            Step::Prepare => matches!(self.test, TestState::Ok(_)),
            Step::Try => self.tried_ok,
            _ => true,
        }
    }
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || self.render_scoped(window, cx).children(self.editor.clone()))
    }
}

impl Onboarding {
    fn render_scoped(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = crate::settings::Ctx::new(&self.st, cx);
        theme::apply(&c.p, cx);
        let p = c.p;
        ui::set_ui_language(c.lang);

        // Moving between steps slides the old page out and the new one in (see `Slide`).
        let slide = self.slide.as_ref().map(|s| (s.from, s.forward, s.reduced, s.at.elapsed().as_secs_f32()));
        let root = div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(crate::ui::cjk_family())
            .capture_any_mouse_down(|_, window, cx| window.blur(cx));

        let Some((from, forward, reduced, t)) = slide else {
            return root.child(self.page(self.step, &c, cx, None));
        };
        if t >= transition::total(reduced) {
            self.slide = None;
            // The page is drawn again as a new element tree: hover state must be read afresh from the pointer.
            self.reset_motion();
            return root.child(self.page(self.step, &c, cx, None));
        }
        window.request_animation_frame();

        let v = transition::view(forward, reduced, t);

        // The leaving page is built under its own motion scope, so controls that both pages share (the footer's
        // buttons) never trade hover state.
        let leaving_scope = self.leaving_motion.handle();
        let leaving = ui::with_motion_scope(leaving_scope, || self.page(from, &c, cx, None));
        let arriving = self.page(self.step, &c, cx, Some(v.art_dx));
        root.child(div().id("page-leaving").absolute().size_full().left(px(v.out_dx)).opacity(v.out_a).child(leaving))
            .child(div().id("page-arriving").absolute().size_full().left(px(v.in_dx)).opacity(v.in_a).child(arriving))
    }

    /// One full-window page for `step`: the welcome page, or a step's content next to its art. `art_dx` offsets the art
    /// panel while the page slides in.
    fn page(&mut self, step: Step, c: &crate::settings::Ctx, cx: &mut Context<Self>, art_dx: Option<f32>) -> Div {
        let p = c.p;
        if step == Step::Welcome {
            return steps::welcome(self, c, cx);
        }
        let current = std::mem::replace(&mut self.step, step);
        let (title, desc, body) = steps::body(self, c, cx);
        let next_label = if step == Step::Try { c.t(Tr::OnboardingDone) } else { c.t(Tr::OnboardingContinue) };
        let can_next = self.can_next(cx);
        let footer = components::footer(step, c, cx, next_label, can_next);
        let art = components::art(step, &p);
        self.step = current;
        let left = div()
            .flex_1()
            .h_full()
            .px(px(56.))
            .pt(px(60.))
            .pb(px(36.))
            .flex()
            .flex_col()
            .gap(px(28.))
            .child(components::stepper(step, &p, c.lang))
            .child(div().flex().flex_col().gap(px(10.)).child(text(title, 28., FontWeight::BOLD, p.text)).child(text(
                desc,
                13.,
                FontWeight::NORMAL,
                p.muted,
            )))
            .child(div().flex_1().w_full().child(body))
            .child(footer);
        let art = match art_dx {
            Some(dx) => art.relative().left(px(dx)),
            None => art,
        };
        div().size_full().flex().child(left).child(art)
    }
}

#[cfg(test)]
mod tests {
    use super::Requests;

    #[test]
    fn only_the_newest_request_may_write_its_result() {
        let mut reqs = Requests::default();
        let first = reqs.begin();
        assert!(reqs.is_current(first));
        let second = reqs.begin();
        assert!(!reqs.is_current(first), "an older request's completion must be dropped");
        assert!(reqs.is_current(second));
    }

    #[test]
    fn any_step_change_supersedes_pending_requests() {
        let mut reqs = Requests::default();
        let req = reqs.begin();
        reqs.bump(); // entering, leaving or restarting a step
        assert!(!reqs.is_current(req));
    }

    #[test]
    fn a_config_edit_supersedes_the_in_flight_request() {
        let mut reqs = Requests::default();
        let req = reqs.begin(); // a test is running against the current endpoint/key/provider
        reqs.bump(); // the user edits that configuration: the result state goes idle
        assert!(!reqs.is_current(req), "the obsolete request's success must not overwrite the fresh idle state");
    }
}

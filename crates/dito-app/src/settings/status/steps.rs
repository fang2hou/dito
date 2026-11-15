//! The steps of a dictation, in the order it runs: what each uses, and what to fix when it cannot run.

use std::rc::Rc;
use std::time::Duration;

use dito_core::config::{Config, EngineConfig, InsertMethod, RecordMode, Source};
use dito_core::error::ErrorKind;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::{Tr, fmt};
use dito_core::secrets::Role;
use dito_platform::{Permission, Shortcut, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::StatusPage;
use crate::settings::{self, Ctx, Page, role};
use crate::theme::Palette;
use crate::ui::{self, BtnKind, Cb, StatusTone, TestState, cbk, text};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dot {
    Ok,
    Attention,
    Changing,
    Off,
}

impl Dot {
    fn color(self, p: &Palette) -> Hsla {
        match self {
            Self::Ok => p.success,
            Self::Attention => p.danger,
            Self::Changing => p.warning_text,
            Self::Off => p.muted,
        }
    }
}

enum Fix {
    /// Opens the permission's System Settings pane.
    OpenSettings(Permission),
    /// Asks for the permission where the system still allows a prompt.
    Allow(Permission),
    Start(String),
    Retry(String),
    ModelSettings,
}

/// What ends the row: a healthy step shows its value, any other state a Status Label.
enum End {
    Value(SharedString),
    /// Seconds the step took in the most recent dictation; `None` until one has run on this engine.
    RecentTime(Option<f32>),
    Off(SharedString),
    Label(SharedString, StatusTone),
}

pub(super) struct Step {
    id: &'static str,
    title: SharedString,
    detail: SharedString,
    pub(super) dot: Dot,
    end: End,
    message: Option<SharedString>,
    fix: Option<Fix>,
    /// System audio is needed to transcribe meetings, not to dictate, so it never decides the verdict.
    pub(super) blocks_dictation: bool,
}

struct Problem {
    label: Tr,
    tone: StatusTone,
    message: SharedString,
    fix: Option<Fix>,
}

impl Step {
    fn healthy(id: &'static str, title: SharedString, detail: SharedString, end: End) -> Self {
        Self { id, title, detail, dot: Dot::Ok, end, message: None, fix: None, blocks_dictation: true }
    }

    fn changing(mut self, c: &Ctx, label: Tr) -> Self {
        self.dot = Dot::Changing;
        self.end = End::Label(c.t(label), StatusTone::Warning);
        self
    }

    fn failing(mut self, c: &Ctx, problem: Problem) -> Self {
        self.dot = Dot::Attention;
        self.end = End::Label(c.t(problem.label), problem.tone);
        self.message = Some(problem.message);
        self.fix = problem.fix;
        self
    }
}

impl StatusPage {
    pub(super) fn steps(&self, c: &Ctx) -> [Step; 6] {
        let cfg = self.st.cfg.get();
        let times = self.st.dictation.step_times();
        [
            self.shortcut_step(c, &cfg),
            self.microphone_step(c, &cfg),
            self.system_audio_step(c),
            self.engine_step(c, Role::Asr, &cfg.asr, times.transcription_secs(&cfg.asr)),
            self.polish_step(c, &cfg, times.polishing_secs(&cfg.text_model)),
            self.insert_step(c, &cfg),
        ]
    }

    pub(super) fn shortcut_label(&self, c: &Ctx) -> String {
        let shortcut = self.st.cfg.get().general.shortcut.clone();
        Shortcut::parse(&shortcut).map(|s| dito_platform::keymap::label(&s, c.lang)).unwrap_or(shortcut)
    }

    fn shortcut_step(&self, c: &Ctx, cfg: &Config) -> Step {
        let detail = match cfg.general.record_mode {
            RecordMode::Toggle => Tr::StatusShortcutToggle,
            RecordMode::Hold => Tr::StatusShortcutHold,
        };
        let step =
            Step::healthy("shortcut", c.t(Tr::StatusShortcut), c.t(detail), End::Value(self.shortcut_label(c).into()));
        if self.accessibility_allowed && self.st.hotkeys.available() {
            return step;
        }
        step.failing(
            c,
            Problem {
                label: Tr::StatusNotAllowed,
                tone: StatusTone::Danger,
                message: c.t(Tr::StatusShortcutBlocked),
                fix: Some(Fix::OpenSettings(Permission::Accessibility)),
            },
        )
    }

    fn microphone_step(&self, c: &Ctx, cfg: &Config) -> Step {
        let device = &cfg.general.microphone;
        let value = if device.is_empty() { c.t(Tr::GeneralAuto) } else { SharedString::from(device.clone()) };
        let step =
            Step::healthy("microphone", c.t(Tr::GeneralMicrophone), c.t(Tr::StatusMicrophoneDetail), End::Value(value));
        if self.microphone_allowed {
            return step;
        }
        step.failing(
            c,
            Problem {
                label: Tr::StatusNotAllowed,
                tone: StatusTone::Danger,
                message: c.t(Tr::StatusMicrophoneBlocked),
                fix: Some(Fix::OpenSettings(Permission::Microphone)),
            },
        )
    }

    fn system_audio_step(&self, c: &Ctx) -> Step {
        let step = Step {
            blocks_dictation: false,
            ..Step::healthy(
                "system-audio",
                c.t(Tr::StatusSystemAudio),
                c.t(Tr::StatusSystemAudioDetail),
                End::Value(c.t(Tr::StatusAllowed)),
            )
        };
        let fix = match self.system_audio {
            Status::Granted => return step,
            // An unanswered permission can still show the system prompt; a refused one only reopens in Settings.
            Status::NotDetermined => Fix::Allow(Permission::SystemAudio),
            Status::Denied => Fix::OpenSettings(Permission::SystemAudio),
        };
        step.failing(
            c,
            Problem {
                label: Tr::StatusNotAllowed,
                tone: StatusTone::Danger,
                message: c.t(Tr::StatusSystemAudioBlocked),
                fix: Some(fix),
            },
        )
    }

    fn polish_step(&self, c: &Ctx, cfg: &Config, recent_secs: Option<f32>) -> Step {
        if cfg.polish.enabled {
            return self.engine_step(c, Role::Text, &cfg.text_model, recent_secs);
        }
        Step {
            dot: Dot::Off,
            ..Step::healthy("polish", c.t(Tr::ModelsPolish), c.t(Tr::StatusPolishOff), End::Off(c.t(Tr::GeneralOff)))
        }
    }

    fn engine_step(&self, c: &Ctx, role: Role, engine: &EngineConfig, recent_secs: Option<f32>) -> Step {
        let (id, title) = match role {
            Role::Asr => ("recognition", Tr::ModelsDictation),
            Role::Text => ("polish", Tr::ModelsPolish),
        };
        let local = engine.source == Source::Local;
        let name = if local { self.local_model_name(&engine.local_model) } else { cloud_model_name(c, role, engine) };
        let detail =
            if local { fmt::status_local_model(c.lang, &name) } else { fmt::status_cloud_model(c.lang, &name) };
        let step = Step::healthy(id, c.t(title), detail.into(), End::RecentTime(recent_secs));
        if local { self.local_state(c, step, &engine.local_model) } else { self.cloud_state(c, step, role) }
    }

    pub(super) fn local_model_name(&self, id: &str) -> String {
        self.st.models.spec(id).map_or_else(|| id.to_owned(), |spec| spec.name.clone())
    }

    fn local_state(&self, c: &Ctx, step: Step, model: &str) -> Step {
        let problem = |label, tone, message, fix| Problem { label, tone, message: c.t(message), fix: Some(fix) };
        match self.st.local.status(model) {
            EngineStatus::Running | EngineStatus::Remote => step,
            EngineStatus::Loading => step.changing(c, Tr::MenubarLoading),
            EngineStatus::Stopping => step.changing(c, Tr::ModelsStopping),
            EngineStatus::Stopped => step.failing(
                c,
                problem(Tr::MenubarStopped, StatusTone::Danger, Tr::StatusModelStopped, Fix::Start(model.to_owned())),
            ),
            EngineStatus::NotInstalled => step.failing(
                c,
                problem(Tr::ModelsNotInstalled, StatusTone::Neutral, Tr::StatusModelMissing, Fix::ModelSettings),
            ),
            EngineStatus::Failed(_) => step.failing(
                c,
                problem(
                    Tr::ModelsStartupError,
                    StatusTone::Danger,
                    Tr::StatusModelLoadFailed,
                    Fix::Retry(model.to_owned()),
                ),
            ),
        }
    }

    fn cloud_state(&self, c: &Ctx, step: Step, role: Role) -> Step {
        let line = match &self.checks[role::index(role)] {
            TestState::Ok(_) => return step,
            TestState::Idle | TestState::Running => return step.changing(c, Tr::ModelsChecking),
            TestState::Failed(line) => line,
        };
        let (label, message) = match ErrorKind::decode(line) {
            Some((ErrorKind::Auth, _)) => (Tr::StatusAuthError, c.t(Tr::StatusCheckApiKey)),
            Some((ErrorKind::Timeout, _)) => (Tr::ModelsConnectionError, c.t(Tr::StatusRequestTimedOut)),
            _ => (Tr::ModelsConnectionError, ui::error_line(c.lang, line).into()),
        };
        step.failing(c, Problem { label, tone: StatusTone::Danger, message, fix: Some(Fix::ModelSettings) })
    }

    fn insert_step(&self, c: &Ctx, cfg: &Config) -> Step {
        let method = match cfg.general.insert_method {
            InsertMethod::KeyInput => Tr::GeneralKeyInput,
            InsertMethod::Paste => Tr::GeneralPaste,
        };
        let step = Step::healthy("insert", c.t(Tr::StatusInsert), c.t(Tr::StatusInsertDetail), End::Value(c.t(method)));
        if self.accessibility_allowed {
            return step;
        }
        // The shortcut row carries the button: one permission, one fix.
        step.failing(
            c,
            Problem {
                label: Tr::StatusNotAllowed,
                tone: StatusTone::Danger,
                message: c.t(Tr::StatusInsertBlocked),
                fix: None,
            },
        )
    }

    fn start_model(&mut self, id: &str, cx: &mut Context<Self>) {
        let (local, id) = (self.st.local.clone(), id.to_owned());
        self.st.rt.spawn(async move {
            local.restart(&id).await;
        });
        // Show the loading state without waiting for the next reading.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn fix_button(&self, c: &Ctx, cx: &mut Context<Self>, step: &'static str, fix: Fix) -> Stateful<Div> {
        let (label, on_click): (Tr, Cb) = match fix {
            Fix::OpenSettings(permission) => {
                (Tr::StatusOpenSystemSettings, cbk(cx, move |_: &mut Self, _, _| permissions::request(permission)))
            }
            Fix::Allow(permission) => {
                (Tr::GeneralAllow, cbk(cx, move |_: &mut Self, _, _| permissions::request(permission)))
            }
            Fix::Start(id) => (Tr::StatusStart, cbk(cx, move |this: &mut Self, _, cx| this.start_model(&id, cx))),
            Fix::Retry(id) => (Tr::StatusRetry, cbk(cx, move |this: &mut Self, _, cx| this.start_model(&id, cx))),
            Fix::ModelSettings => (
                Tr::StatusOpenModelSettings,
                Rc::new(|_, cx| {
                    let st = settings::global_state(cx);
                    settings::open(st, Page::Models, cx);
                }),
            ),
        };
        ui::button(SharedString::from(format!("fix-{step}")), &c.p, &c.t(label), BtnKind::Secondary, None, on_click)
    }

    pub(super) fn step_row(&self, c: &Ctx, cx: &mut Context<Self>, step: Step) -> Div {
        let p = c.p;
        let line =
            |s: SharedString, color| text(s, 13., FontWeight::NORMAL, color).whitespace_normal().line_height(px(20.));
        let end = match step.end {
            End::Value(value) => vec![value_text(value, p.text)],
            End::Off(value) => vec![value_text(value, p.muted)],
            End::RecentTime(secs) => {
                let value = secs.map_or_else(|| "—".to_owned(), |s| format!("{s:.1} {}", c.t(Tr::UnitsSeconds)));
                vec![
                    text(c.t(Tr::StatusRecentTime), 12., FontWeight::NORMAL, p.muted).line_height(px(20.)),
                    value_text(value.into(), p.text),
                ]
            }
            End::Label(label, tone) => vec![ui::status_chip(&p, label, tone)],
        };
        let body = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(text(step.title, 15., FontWeight::MEDIUM, p.text).line_height(px(22.)))
            .child(line(step.detail, p.muted))
            .children(step.message.map(|message| line(message, p.text)))
            .children(
                step.fix.map(|fix| div().pt(px(8.)).pb(px(2.)).flex().child(self.fix_button(c, cx, step.id, fix))),
            );
        div()
            .w_full()
            .px(px(24.))
            .py(px(16.))
            .flex()
            .items_start()
            .gap(px(14.))
            .child(
                div()
                    .h(px(22.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .child(div().size(px(10.)).rounded_full().bg(step.dot.color(&p))),
            )
            .child(body)
            .child(div().h(px(24.)).flex().flex_none().items_center().gap(px(10.)).children(end))
    }
}

fn value_text(value: SharedString, color: Hsla) -> Div {
    text(value, 13., FontWeight::MEDIUM, color).line_height(px(20.))
}

/// The model id the configuration sends, or the configuration's name when it has none.
fn cloud_model_name(c: &Ctx, role: Role, engine: &EngineConfig) -> String {
    let asr = role::cloud_asr_configurations(role, engine);
    let polish = engine.text_configurations(c.lang);
    let (model, name) = match asr.as_deref() {
        Some(asr) => (
            asr.active().and_then(|active| active.config["connection"]["model"].as_str()).unwrap_or(""),
            asr.active_name(),
        ),
        None => {
            let active = polish.active();
            (active.map_or("", |active| active.target.model()), active.map(|active| active.name.as_str()))
        }
    };
    if model.is_empty() { name.unwrap_or("—") } else { model }.to_owned()
}

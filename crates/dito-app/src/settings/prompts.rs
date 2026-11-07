use std::sync::Arc;

use dito_core::i18n::{Lang, Tr};
use dito_core::prompts::{Prompts, Style};
use dito_platform::system::{InstalledApplication, installed_application};

mod application_ui;
mod view;
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Ctx;
use crate::services;
use crate::state::AppState;
use crate::ui;

enum Try {
    Idle,
    Running,
    Done(String, u64),
    Failed(String),
}

pub struct PromptsPage {
    st: Arc<AppState>,
    lang: Lang,
    style: Style,
    base: Entity<TextareaState>,
    instruction: Entity<TextareaState>,
    samples: Vec<Entity<InputState>>,
    try_in: Entity<TextareaState>,
    try_state: Try,
    dirty: bool,
    preview_revision: u64,
    drafts: Prompts,
    apps: Vec<AppChoice>,
    menu_open: bool,
    copy_menu_open: bool,
    dialog: Option<Dialog>,
    target: Option<InstalledApplication>,
    target_icon: Option<Arc<Image>>,
    copy_source: Style,
    dialog_error: Option<String>,
    dialog_focus: FocusHandle,
    help_focus: [FocusHandle; 2],
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

fn textarea(window: &mut Window, cx: &mut Context<PromptsPage>, value: &str) -> Entity<TextareaState> {
    let v = value.to_string();
    cx.new(|cx| TextareaState::new(window, cx).default_value(v))
}

impl PromptsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let (lang, style) = (cfg.general.lang(), Style::General);
        let set = cfg.prompts.set(lang);
        let base = textarea(window, cx, &set.base);
        let instruction = textarea(window, cx, &set.style(&style).instruction);
        let try_in = textarea(window, cx, dito_core::i18n::sample_utterance(lang));
        for ent in [&base, &instruction, &try_in] {
            let handle = ent.read(cx).focus_handle(cx);
            cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()).detach();
            cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()).detach();
        }
        for ent in [&base, &instruction] {
            cx.subscribe_in(ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.changed();
                    cx.notify();
                }
            })
            .detach();
        }
        cx.subscribe_in(&try_in, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.preview_revision += 1;
                this.try_state = Try::Idle;
                cx.notify();
            }
        })
        .detach();
        let mut page = Self {
            motion: ui::MotionScope::new(),
            st,
            lang,
            style,
            base,
            instruction,
            samples: Vec::new(),
            try_in,
            try_state: Try::Idle,
            drafts: cfg.prompts.clone(),
            apps: app_choices(&cfg.prompts),
            dirty: false,
            preview_revision: 0,
            menu_open: false,
            copy_menu_open: false,
            dialog: None,
            target: None,
            target_icon: None,
            copy_source: Style::General,
            dialog_error: None,
            dialog_focus: cx.focus_handle(),
            help_focus: [cx.focus_handle().tab_stop(true), cx.focus_handle().tab_stop(true)],
        };
        for handle in &page.help_focus {
            cx.on_focus_in(handle, window, |_, _, cx| cx.notify()).detach();
            cx.on_focus_out(handle, window, |_, _, _, cx| cx.notify()).detach();
        }
        page.rebuild_samples(window, cx);
        page
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.preview_revision += 1;
        self.try_state = Try::Idle;
    }

    fn rebuild_samples(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.drafts.set(self.lang).style(&self.style).samples.clone();
        self.samples = list.into_iter().map(|s| self.sample_input(&s, window, cx)).collect();
    }

    fn sample_input(&self, value: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        let v = value.to_string();
        let ent = cx.new(|cx| InputState::new(window, cx).default_value(v));
        ui::notify_on_focus(&ent, window, cx);
        cx.subscribe_in(&ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.changed();
                cx.notify();
            }
        })
        .detach();
        ent
    }

    /// Current edits applied to a copy of the saved prompts.
    fn edited(&self, cx: &App) -> Prompts {
        let mut prompts = self.drafts.clone();
        let set = prompts.set_mut(self.lang);
        set.base = self.base.read(cx).value().to_string();
        let sp = set.style_mut(&self.style);
        sp.instruction = self.instruction.read(cx).value().to_string();
        sp.samples =
            self.samples.iter().map(|s| s.read(cx).value().to_string()).filter(|s| !s.trim().is_empty()).collect();
        prompts
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let edited = self.edited(cx);
        self.drafts = edited.clone();
        services::save_config(&self.st, |c| c.prompts = edited);
        self.dirty = false;
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.dirty;
        let set = self.drafts.set(self.lang);
        let (base, instr) = (set.base.clone(), set.style(&self.style).instruction.clone());
        self.base.update(cx, |s, cx| s.set_value(base, window, cx));
        self.instruction.update(cx, |s, cx| s.set_value(instr, window, cx));
        self.rebuild_samples(window, cx);
        self.dirty = dirty;
        self.try_state = Try::Idle;
        cx.notify();
    }

    fn switch(&mut self, lang: Lang, style: Style, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts = self.edited(cx);
        self.preview_revision += 1;
        self.menu_open = false;
        if self.lang != lang {
            // Keep the try-it sentence in the prompt language unless the user wrote their own.
            let old = dito_core::i18n::sample_utterance(self.lang);
            if self.try_in.read(cx).value().as_ref() == old {
                let new = dito_core::i18n::sample_utterance(lang);
                self.try_in.update(cx, |s, cx| s.set_value(new, window, cx));
            }
        }
        self.lang = lang;
        self.style = style;
        self.load(window, cx);
    }

    fn run_try(&mut self, cx: &mut Context<Self>) {
        let text = self.try_in.read(cx).value().to_string();
        if text.trim().is_empty() || matches!(self.try_state, Try::Running) {
            return;
        }
        self.try_state = Try::Running;
        cx.notify();
        let (st, prompts, style, lang) = (self.st.clone(), self.edited(cx), self.style.clone(), self.lang);
        let request_style = style.clone();
        let revision = self.preview_revision;
        cx.spawn(async move |this, cx| {
            let r = services::try_polish(st, prompts, style, lang, text).await;
            let _ = this.update(cx, |t, cx| {
                if t.preview_revision != revision || t.style != request_style || t.lang != lang {
                    return;
                }
                t.try_state = match r {
                    Ok((out, ms)) => Try::Done(out, ms),
                    Err(e) => Try::Failed(e),
                };
                cx.notify();
            });
        })
        .detach();
    }
}

impl PromptsPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.menu_open = false;
        self.copy_menu_open = false;
        self.dialog = None;
        self.motion.reset();
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for PromptsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(window, cx))
    }
}

#[derive(Clone)]
struct AppChoice {
    style: Style,
    name: String,
    icon: Option<Arc<Image>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    Add,
    Reset,
    RemoveApp,
}

fn app_choices(prompts: &Prompts) -> Vec<AppChoice> {
    let mut choices = Vec::new();
    for id in ["com.tinyspeck.slackmacgap", "com.microsoft.teams2", "com.microsoft.teams", "dev.herdr.app"]
        .into_iter()
        .chain(prompts.applications.keys().map(String::as_str))
    {
        // A configured app stays in the list even when the bundle is not installed, so its settings remain selectable
        // (and removable) instead of silently disappearing from the picker.
        let (name, icon) = match installed_application(id) {
            Some(app) => (
                app.name,
                (!app.icon_png.is_empty()).then(|| Arc::new(Image::from_bytes(ImageFormat::Png, app.icon_png))),
            ),
            None => match prompts.applications.get(id) {
                Some(name) => (name.clone(), None),
                None => continue,
            },
        };
        let style = prompts.for_application(id);
        if choices.iter().any(|c: &AppChoice| c.style == style) {
            continue;
        }
        choices.push(AppChoice { style, name, icon });
    }
    choices.push(AppChoice { style: Style::General, name: String::new(), icon: None });
    choices
}

#[cfg(feature = "screenshots")]
impl PromptsPage {
    pub(in crate::settings) fn preview(&mut self, case: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        if self.lang != Lang::Ja {
            self.switch(Lang::Ja, self.style.clone(), window, cx);
        }
        self.menu_open = false;
        self.copy_menu_open = false;
        ui::force_hover(&self.motion.handle(), ElementId::from("prompt-examples-help"), false);
        match case {
            "menu" => self.menu_open = true,
            "add" => {
                self.open_dialog(Dialog::Add, window, cx);
                self.target = installed_application("com.apple.TextEdit");
                self.target_icon =
                    self.target.as_ref().map(|a| Arc::new(Image::from_bytes(ImageFormat::Png, a.icon_png.clone())));
            }
            "reset" => self.open_dialog(Dialog::Reset, window, cx),
            "help" => ui::force_hover(&self.motion.handle(), ElementId::from("prompt-examples-help"), true),
            "herdr" => self.switch(self.lang, Style::Herdr, window, cx),
            "custom" | "removeapp" => {
                self.drafts.add_application("com.apple.AppStore", "App Store", &Style::General);
                self.apps = app_choices(&self.drafts);
                self.style = self.drafts.for_application("com.apple.AppStore");
                self.load(window, cx);
                if case == "removeapp" {
                    self.open_dialog(Dialog::RemoveApp, window, cx);
                }
            }
            _ => self.switch(self.lang, Style::General, window, cx),
        }
        cx.notify();
    }
}

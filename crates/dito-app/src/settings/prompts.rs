use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::i18n::{Lang, Tr};
use dito_core::prompts::{AppTarget, BUILTIN_APP_RULES, Prompts};
use dito_platform::system::{InstalledApplication, installed_application};

mod application_ui;
mod history;
use history::{Editor, History, Snapshot};
mod view;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Ctx;
use crate::services;
use crate::state::AppState;
use crate::ui::{self, InputEvent, Textarea, TextareaState};

enum Try {
    Idle,
    Running,
    Done(u64),
    Failed(String),
}

pub struct PromptsPage {
    st: Arc<AppState>,
    lang: Lang,
    target: AppTarget,
    base: Entity<TextareaState>,
    instruction: Entity<TextareaState>,
    samples: Vec<Entity<TextareaState>>,
    try_in: Entity<TextareaState>,
    try_out: Entity<TextareaState>,
    placeholder_lang: Lang,
    try_state: Try,
    histories: HashMap<(Lang, Option<AppTarget>), History>,
    save_revision: u64,
    save_error: Option<String>,
    group_started: [Option<Instant>; 2],
    preview_revision: u64,
    drafts: Prompts,
    apps: Vec<AppChoice>,
    menu_open: bool,
    copy_menu_open: bool,
    dialog: Option<Dialog>,
    picked_app: Option<InstalledApplication>,
    picked_icon: Option<Arc<Image>>,
    copy_source: AppTarget,
    dialog_error: Option<String>,
    dialog_focus: FocusHandle,
    help_focus: [FocusHandle; 2],
    motion: ui::MotionScope,
    #[cfg(feature = "screenshots")]
    edit_probe: Option<(Snapshot, Option<Snapshot>)>,
}

fn textarea(window: &mut Window, cx: &mut Context<PromptsPage>, value: &str) -> Entity<TextareaState> {
    let v = value.to_string();
    cx.new(|cx| TextareaState::new(window, cx).default_value(v))
}

impl PromptsPage {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        services::watch_config(&st, cx);
        let cfg = st.cfg.get();
        let (lang, initial_target) = (cfg.general.lang(), AppTarget::General);
        let set = cfg.prompts.set(lang);
        let base = textarea(window, cx, &set.base);
        let instruction = textarea(window, cx, &set.app_rules(&initial_target).instruction);
        let try_in = textarea(window, cx, dito_core::i18n::sample_utterance(lang));
        let placeholder = SharedString::from(lang.tr(Tr::PromptsPreviewPlaceholder));
        let try_out = cx.new(|cx| TextareaState::new(window, cx).placeholder(placeholder));
        for ent in [&base, &instruction, &try_in, &try_out] {
            let handle = ent.read(cx).focus_handle(cx);
            cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()).detach();
            cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()).detach();
        }
        for (editor, ent) in [(Editor::Common, &base), (Editor::Application, &instruction)] {
            cx.subscribe_in(ent, window, move |this: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.changed(editor, cx);
                    cx.notify();
                }
                if matches!(ev, InputEvent::Blur) {
                    this.close_group(editor, cx);
                }
            })
            .detach();
        }
        cx.subscribe_in(&try_in, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.preview_revision += 1;
                // The result stays until the next run replaces it; only an unfinished or failed run is dropped.
                if !matches!(this.try_state, Try::Done(_)) {
                    this.try_state = Try::Idle;
                }
                cx.notify();
            }
        })
        .detach();
        let mut page = Self {
            motion: ui::MotionScope::new(),
            #[cfg(feature = "screenshots")]
            edit_probe: None,
            st,
            lang,
            target: initial_target,
            base,
            instruction,
            samples: Vec::new(),
            try_in,
            try_out,
            placeholder_lang: lang,
            try_state: Try::Idle,
            drafts: cfg.prompts.clone(),
            apps: app_choices(&cfg.prompts),
            histories: HashMap::new(),
            save_revision: 0,
            save_error: None,
            group_started: [None, None],
            preview_revision: 0,
            menu_open: false,
            copy_menu_open: false,
            dialog: None,
            picked_app: None,
            picked_icon: None,
            copy_source: AppTarget::General,
            dialog_error: None,
            dialog_focus: cx.focus_handle(),
            help_focus: [cx.focus_handle().tab_stop(true), cx.focus_handle().tab_stop(true)],
        };
        for handle in &page.help_focus {
            cx.on_focus_in(handle, window, |_, _, cx| cx.notify()).detach();
            cx.on_focus_out(handle, window, |_, _, _, cx| cx.notify()).detach();
        }
        page.ensure_histories();
        page.rebuild_samples(window, cx);
        page
    }

    fn history_key(&self, editor: Editor) -> (Lang, Option<AppTarget>) {
        (self.lang, (editor == Editor::Application).then(|| self.target.clone()))
    }

    fn snapshot(&self, editor: Editor) -> Snapshot {
        let set = self.drafts.set(self.lang);
        match editor {
            Editor::Common => Snapshot::Common(set.base.clone()),
            Editor::Application => Snapshot::Application(set.app_rules(&self.target).clone()),
        }
    }

    fn ensure_histories(&mut self) {
        for editor in [Editor::Common, Editor::Application] {
            let key = self.history_key(editor);
            let initial = self.snapshot(editor);
            self.histories.entry(key).or_insert_with(|| History::new(initial));
        }
    }

    fn changed(&mut self, editor: Editor, cx: &mut Context<Self>) {
        // InputEvent::Change is emitted for committed input, never marked IME text.
        match editor {
            Editor::Common => self.drafts.set_mut(self.lang).base = self.base.read(cx).value().to_string(),
            Editor::Application => {
                let prompt = self.drafts.set_mut(self.lang).app_rules_mut(&self.target);
                prompt.instruction = self.instruction.read(cx).value().to_string();
                prompt.samples = self.samples.iter().map(|s| s.read(cx).value().to_string()).collect();
            }
        }
        let key = self.history_key(editor);
        let snapshot = self.snapshot(editor);
        self.histories.get_mut(&key).expect("active editor history").change(snapshot);
        let slot = editor as usize;
        if self.group_started[slot].is_some_and(|start| start.elapsed() >= Duration::from_secs(15)) {
            self.histories.get_mut(&key).unwrap().checkpoint(false);
            self.group_started[slot] = None;
        }
        self.group_started[slot].get_or_insert_with(Instant::now);
        self.preview_revision += 1;
        self.try_state = Try::Idle;
        self.schedule_save(cx);
        let revision = self.save_revision;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                if this.save_revision == revision {
                    if let Some(history) = this.histories.get_mut(&key) {
                        history.checkpoint(false);
                    }
                    this.group_started[slot] = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_revision += 1;
        let revision = self.save_revision;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this.save_revision == revision {
                    this.save(cx);
                }
            });
        })
        .detach();
    }

    fn close_group(&mut self, editor: Editor, cx: &mut Context<Self>) {
        let key = self.history_key(editor);
        if let Some(history) = self.histories.get_mut(&key) {
            history.checkpoint(false);
        }
        self.group_started[editor as usize] = None;
        let saved = self.st.cfg.get();
        let dirty = match editor {
            Editor::Common => self.drafts.set(self.lang).base != saved.prompts.set(self.lang).base,
            Editor::Application => {
                self.drafts.set(self.lang).app_rules(&self.target)
                    != saved.prompts.set(self.lang).app_rules(&self.target)
            }
        };
        if dirty {
            self.save(cx);
        }
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, window: &mut Window, cx: &mut Context<Self>) {
        match snapshot {
            Snapshot::Common(value) => {
                self.drafts.set_mut(self.lang).base = value.clone();
                self.base.update(cx, |state, cx| {
                    let selection = state.selected_range();
                    state.set_value(value, window, cx);
                    state.set_selected_range(selection, cx);
                });
            }
            Snapshot::Application(value) => {
                let sample_focus = self.samples.iter().enumerate().find_map(|(index, input)| {
                    input
                        .read(cx)
                        .focus_handle(cx)
                        .contains_focused(window, cx)
                        .then(|| (index, input.read(cx).selected_range()))
                });
                *self.drafts.set_mut(self.lang).app_rules_mut(&self.target) = value.clone();
                self.instruction.update(cx, |state, cx| {
                    let selection = state.selected_range();
                    state.set_value(value.instruction, window, cx);
                    state.set_selected_range(selection, cx);
                });
                self.rebuild_samples(window, cx);
                if let Some((index, selection)) = sample_focus {
                    if let Some(input) = self.samples.get(index.min(self.samples.len().saturating_sub(1))) {
                        input.update(cx, |state, cx| state.set_selected_range(selection, cx));
                        input.read(cx).focus_handle(cx).focus(window, cx);
                    } else {
                        self.instruction.read(cx).focus_handle(cx).focus(window, cx);
                    }
                }
            }
        }
        self.preview_revision += 1;
        self.try_state = Try::Idle;
        self.schedule_save(cx);
    }

    fn undo(&mut self, editor: Editor, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.history_key(editor);
        if let Some(snapshot) = self.histories.get_mut(&key).and_then(History::undo) {
            self.apply_snapshot(snapshot, window, cx);
        }
        cx.notify();
    }

    fn redo(&mut self, editor: Editor, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.history_key(editor);
        if let Some(snapshot) = self.histories.get_mut(&key).and_then(History::redo) {
            self.apply_snapshot(snapshot, window, cx);
        }
        cx.notify();
    }

    fn restore(&mut self, editor: Editor, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.history_key(editor);
        let snapshot = match editor {
            Editor::Common => Snapshot::Common(Prompts::default().set(self.lang).base.clone()),
            Editor::Application => Snapshot::Application(self.drafts.default_app_rules(self.lang, &self.target)),
        };
        let history = self.histories.get_mut(&key).unwrap();
        history.checkpoint(true);
        history.change(snapshot.clone());
        history.checkpoint(true);
        self.apply_snapshot(snapshot, window, cx);
        self.dialog = None;
        self.save(cx);
    }

    fn rebuild_samples(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.drafts.set(self.lang).app_rules(&self.target).samples.clone();
        self.samples = list.into_iter().map(|s| self.sample_input(&s, window, cx)).collect();
    }

    fn sample_input(&self, value: &str, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextareaState> {
        let v = value.to_string();
        let ent = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, usize::MAX).default_value(v));
        let handle = ent.read(cx).focus_handle(cx);
        cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()).detach();
        cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()).detach();
        cx.subscribe_in(&ent, window, |this: &mut Self, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.changed(Editor::Application, cx);
                cx.notify();
            }
            if matches!(ev, InputEvent::Blur) {
                this.close_group(Editor::Application, cx);
            }
        })
        .detach();
        ent
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let prompts = self.drafts.clone();
        self.save_error = self.st.cfg.update(|config| config.prompts = prompts).err().map(|error| {
            tracing::warn!("could not save prompt drafts: {error}");
            error.to_string()
        });
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let set = self.drafts.set(self.lang);
        let (base, instr) = (set.base.clone(), set.app_rules(&self.target).instruction.clone());
        self.base.update(cx, |s, cx| s.set_value(base, window, cx));
        self.instruction.update(cx, |s, cx| s.set_value(instr, window, cx));
        self.rebuild_samples(window, cx);
        self.ensure_histories();
        self.try_out.update(cx, |s, cx| s.set_value("", window, cx));
        self.try_state = Try::Idle;
        cx.notify();
    }

    fn switch(&mut self, lang: Lang, target: AppTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.close_group(Editor::Common, cx);
        self.close_group(Editor::Application, cx);
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
        self.target = target;
        self.load(window, cx);
    }

    fn run_try(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.try_in.read(cx).value().to_string();
        if text.trim().is_empty() || matches!(self.try_state, Try::Running) {
            return;
        }
        self.try_state = Try::Running;
        cx.notify();
        let (st, prompts, target, lang) = (self.st.clone(), self.drafts.clone(), self.target.clone(), self.lang);
        let request_target = target.clone();
        let revision = self.preview_revision;
        cx.spawn_in(window, async move |this, cx| {
            let r = services::try_polish(st, prompts, target, lang, text).await;
            let _ = this.update_in(cx, |t, window, cx| {
                if t.preview_revision != revision || t.target != request_target || t.lang != lang {
                    return;
                }
                t.try_state = match r {
                    Ok((out, ms)) => {
                        t.try_out.update(cx, |s, cx| s.set_value(out, window, cx));
                        Try::Done(ms)
                    }
                    Err(e) => Try::Failed(e),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn copy_result(&self, cx: &App) -> bool {
        let text = self.try_out.read(cx).value();
        if text.is_empty() {
            return false;
        }
        let copied = dito_platform::insert::copy_to_clipboard(&text);
        if let Err(e) = &copied {
            tracing::warn!("copy failed: {e}");
        }
        copied.is_ok()
    }

    fn sync_result_placeholder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lang = self.st.cfg.get().general.lang();
        if self.placeholder_lang != lang {
            self.placeholder_lang = lang;
            let placeholder = SharedString::from(lang.tr(Tr::PromptsPreviewPlaceholder));
            self.try_out.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
        }
    }
}

impl PromptsPage {
    pub(super) fn reset_motion(&mut self, cx: &mut Context<Self>) {
        self.close_group(Editor::Common, cx);
        self.close_group(Editor::Application, cx);
        self.menu_open = false;
        self.copy_menu_open = false;
        self.dialog = None;
        self.motion.reset();
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for PromptsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Own scope: after a page switch the controls mount statically instead of sliding to values remembered from an
        // earlier visit.
        self.sync_result_placeholder(window, cx);
        ui::render_with_scope(self.motion.handle(), || self.view(window, cx))
    }
}

#[derive(Clone)]
struct AppChoice {
    target: AppTarget,
    name: String,
    icon: Option<Arc<Image>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    Add,
    Reset(Editor),
    RemoveApp,
}

fn app_choices(prompts: &Prompts) -> Vec<AppChoice> {
    let mut choices = Vec::new();
    for id in BUILTIN_APP_RULES
        .iter()
        .flat_map(|rules| rules.bundle_ids.iter().copied())
        .chain(prompts.applications.keys().map(String::as_str))
    {
        // Keep configured apps selectable (and removable) even when their bundle is currently uninstalled.
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
        let target = prompts.for_application(id);
        if target == AppTarget::General || choices.iter().any(|c: &AppChoice| c.target == target) {
            continue;
        }
        choices.push(AppChoice { target, name, icon });
    }
    choices.push(AppChoice { target: AppTarget::General, name: String::new(), icon: None });
    choices
}

#[cfg(feature = "screenshots")]
impl PromptsPage {
    pub(in crate::settings) fn preview(&mut self, case: &str, window: &mut Window, cx: &mut Context<Self>) {
        let preserve_controls =
            matches!(case, "probe-blur" | "probe-menu-open" | "probe-menu-closed" | "probe-add-open");
        if !preserve_controls {
            self.dialog = None;
        }
        if self.lang != Lang::Ja {
            self.switch(Lang::Ja, self.target.clone(), window, cx);
        }
        if !preserve_controls {
            self.menu_open = false;
            self.copy_menu_open = false;
        }
        ui::force_hover(&self.motion.handle(), ElementId::from("prompt-examples-help"), false);
        match case {
            "probe-blur" => {
                assert!(!self.base.read(cx).focus_handle(cx).contains_focused(window, cx));
                assert!(
                    !self.instruction.read(cx).focus_handle(cx).contains_focused(window, cx),
                    "outside click must end editing"
                );
            }
            "probe-menu-open" => assert!(self.menu_open, "application select must open on click"),
            "probe-menu-closed" => assert!(!self.menu_open, "clicking the open trigger must close it"),
            "probe-add-open" => {
                assert!(self.dialog == Some(Dialog::Add));
                assert!(!self.instruction.read(cx).focus_handle(cx).contains_focused(window, cx));
            }
            "probe-ime" => {
                use gpui_kit::EntityInputHandler;
                self.edit_probe = Some((self.snapshot(Editor::Application), None));
                self.instruction.update(cx, |state, cx| {
                    state.focus_handle(cx).focus(window, cx);
                    state.replace_and_mark_text_in_range(None, "marked draft", None, window, cx);
                });
            }
            "probe-input" => {
                use gpui_kit::EntityInputHandler;
                assert_eq!(
                    self.snapshot(Editor::Application),
                    self.edit_probe.as_ref().unwrap().0,
                    "IME marked text must stay out of drafts"
                );
                let text = format!("Committed editing revision {}", self.preview_revision);
                self.instruction.update(cx, |state, cx| state.replace_text_in_range(None, &text, window, cx));
            }
            "probe-not-saved" => {
                let persisted =
                    Snapshot::Application(self.st.cfg.get().prompts.set(self.lang).app_rules(&self.target).clone());
                assert_eq!(persisted, self.edit_probe.as_ref().unwrap().0, "a short thinking pause must not save");
                assert_ne!(
                    self.snapshot(Editor::Application),
                    persisted,
                    "committed input must remain an unsaved draft before 3 seconds"
                );
            }
            "probe-undo" => {
                let edited = self.snapshot(Editor::Application);
                assert_ne!(edited, self.edit_probe.as_ref().unwrap().0, "native committed input must update drafts");
                assert_eq!(
                    self.drafts.set(self.lang).app_rules(&self.target),
                    self.st.cfg.get().prompts.set(self.lang).app_rules(&self.target),
                    "committed drafts must autosave"
                );
                self.edit_probe.as_mut().unwrap().1 = Some(edited);
                window.dispatch_action(Box::new(gpui_kit::base::input::Undo), cx);
            }
            "probe-redo" => {
                assert_eq!(
                    self.snapshot(Editor::Application),
                    self.edit_probe.as_ref().unwrap().0,
                    "native Undo must use grouped history"
                );
                window.dispatch_action(Box::new(gpui_kit::base::input::Redo), cx);
            }
            "probe-reset" => {
                let edited = self.edit_probe.as_ref().unwrap().1.clone().unwrap();
                assert_eq!(self.snapshot(Editor::Application), edited, "native Redo must restore the same edit");
                let common = self.drafts.set(self.lang).base.clone();
                let other_language = if self.lang == Lang::En { Lang::Ja } else { Lang::En };
                let other = self.drafts.set(other_language).clone();
                self.restore(Editor::Application, window, cx);
                assert_eq!(self.drafts.set(self.lang).base, common);
                assert_eq!(self.drafts.set(other_language), &other, "reset must not alter another language");
                self.undo(Editor::Application, window, cx);
                assert_eq!(self.snapshot(Editor::Application), edited, "reset must be one undoable replacement");
                self.save(cx);
            }
            "probe-save-failure" => {
                assert!(
                    self.st.opts.mock && self.st.opts.screenshots.is_some(),
                    "save failure probe requires isolated screenshot data"
                );
                let path = self.st.cfg.path().to_owned();
                let backup = path.with_extension("probe-backup");
                let persisted = self.st.cfg.get().prompts.clone();
                std::fs::rename(&path, &backup).unwrap();
                std::fs::create_dir(&path).unwrap();
                self.instruction
                    .update(cx, |state, cx| state.set_value("A retained draft after a failed write", window, cx));
                self.changed(Editor::Application, cx);
                self.save(cx);
                let failed = self.save_error.is_some();
                std::fs::remove_dir(&path).unwrap();
                std::fs::rename(backup, path).unwrap();
                assert!(failed, "failed persistence must expose an unsaved state");
                assert_eq!(
                    self.st.cfg.get().prompts,
                    persisted,
                    "failed writes must retain the last saved configuration"
                );
                assert_ne!(
                    self.drafts.set(self.lang).app_rules(&self.target),
                    persisted.set(self.lang).app_rules(&self.target)
                );
            }
            "probe-retry" => {
                self.save(cx);
                assert!(self.save_error.is_none());
                assert_eq!(self.st.cfg.get().prompts, self.drafts, "retry must save the retained draft");
            }
            "menu" => self.menu_open = true,
            "add-empty" => self.open_dialog(Dialog::Add, window, cx),
            "add" => {
                self.open_dialog(Dialog::Add, window, cx);
                self.picked_app = installed_application("com.apple.TextEdit");
                self.picked_icon =
                    self.picked_app.as_ref().map(|a| Arc::new(Image::from_bytes(ImageFormat::Png, a.icon_png.clone())));
            }
            "reset" => self.open_dialog(Dialog::Reset(Editor::Common), window, cx),
            "reset-app" => self.open_dialog(Dialog::Reset(Editor::Application), window, cx),
            "history" => {
                let key = self.history_key(Editor::Application);
                self.histories.get_mut(&key).unwrap().checkpoint(true);
                let initial = self.snapshot(Editor::Application);
                self.instruction.update(cx, |state, cx| {
                    state.set_value(
                        format!("A revised application instruction for preview, revision {}.", self.preview_revision),
                        window,
                        cx,
                    )
                });
                self.changed(Editor::Application, cx);
                let edited = self.snapshot(Editor::Application);
                self.undo(Editor::Application, window, cx);
                assert_eq!(self.snapshot(Editor::Application), initial);
                self.redo(Editor::Application, window, cx);
                assert_eq!(self.snapshot(Editor::Application), edited);
            }
            "preview" => {
                let sample = dito_core::i18n::sample_utterance(self.lang);
                self.try_out.update(cx, |s, cx| s.set_value(sample, window, cx));
                self.try_state = Try::Done(420);
            }
            "help" => ui::force_hover(&self.motion.handle(), ElementId::from("prompt-examples-help"), true),
            "herdr" => {
                self.switch(self.lang, AppTarget::builtin("herdr").expect("built-in app rules"), window, cx);
                let one_line = dito_core::i18n::sample_utterance(self.lang).to_owned();
                let several_lines = self.drafts.default_app_rules(self.lang, &self.target).instruction;
                for example in [one_line, several_lines] {
                    let input = self.sample_input(&example, window, cx);
                    self.samples.push(input);
                }
            }
            "custom" | "removeapp" => {
                self.drafts.add_application("com.apple.AppStore", "App Store", &AppTarget::General);
                self.apps = app_choices(&self.drafts);
                self.target = self.drafts.for_application("com.apple.AppStore");
                self.load(window, cx);
                if case == "removeapp" {
                    self.open_dialog(Dialog::RemoveApp, window, cx);
                }
            }
            _ => self.switch(self.lang, AppTarget::General, window, cx),
        }
        cx.notify();
    }
}

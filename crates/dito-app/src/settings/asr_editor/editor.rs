use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::CloudAsrConfiguration;
use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity, Position};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::Value;

use super::configuration_list::{self, KeyChange};
use super::drafts::Draft;
use super::validation::{check, error_row};
use crate::state::AppState;
use crate::ui::{self, EditorState, InputEvent, InputState};

mod configuration_tiles;
mod edit_panels;
mod modal;
#[cfg(feature = "screenshots")]
mod probe;
#[cfg(feature = "screenshots")]
pub use probe::{CloudAsrDraftState, CloudAsrFormState, EditorCommand};

const ISO_CODES: [(&str, &str); 4] = [("ja", "ja"), ("en", "en"), ("zh", "zh"), ("other", "en")];
const LOCALE_CODES: [(&str, &str); 4] = [("ja", "ja-JP"), ("en", "en-US"), ("zh", "zh-CN"), ("other", "en-US")];
const CHECK_DELAY: Duration = Duration::from_millis(200);

pub struct Changed;

/// Completion event for the onboarding wizard's legacy advanced editor.
pub struct Closed(pub bool);

/// How a frozen editor is set up for screenshots.
#[cfg(feature = "screenshots")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorPreview {
    Valid,
    End,
    Invalid,
    Help,
}

/// The cloud speech form: the configurations as tiles, and the selected one's name, key and request JSON.
pub struct AsrEditor {
    st: Arc<AppState>,
    /// What each configuration that was shown keeps, by id.
    drafts: HashMap<String, Draft>,
    /// The id of the configuration the form shows.
    shown: String,
    name: Entity<InputState>,
    key: Entity<InputState>,
    tiles: ui::TileRowState,
    /// Whether the menu of the plus button is open.
    menu: bool,
    /// The configuration a delete dialog asks about, while it is open.
    deleting: Option<configuration_tiles::ConfigurationDeletion>,
    delete_focus: FocusHandle,
    /// The last save failed: the banner above the card says so until it is closed or a save works.
    save_failed: bool,
    help_focus: FocusHandle,
    help: bool,
    dialog: bool,
    help_scroll: ScrollHandle,
    motion: ui::MotionScope,
}

impl EventEmitter<Changed> for AsrEditor {}
impl EventEmitter<Closed> for AsrEditor {}

impl AsrEditor {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::services::watch_config(&st, cx);
        let name = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::Change => this.name_edited(cx),
            InputEvent::Blur => this.name_left(window, cx),
            _ => {}
        })
        .detach();
        let key = cx.new(|cx| InputState::new(window, cx).masked(true));
        cx.subscribe_in(&key, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.key_edited(cx);
            }
        })
        .detach();
        let mut editor = Self {
            st,
            drafts: HashMap::new(),
            shown: String::new(),
            name,
            key,
            tiles: ui::TileRowState::new(),
            menu: false,
            deleting: None,
            delete_focus: cx.focus_handle(),
            save_failed: false,
            help_focus: cx.focus_handle(),
            help: false,
            dialog: false,
            help_scroll: ScrollHandle::new(),
            motion: ui::MotionScope::new(),
        };
        editor.sync(window, cx);
        editor
    }

    /// The onboarding wizard keeps its advanced editor within its existing single overlay.
    pub fn new_dialog(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new(st, window, cx);
        editor.dialog = true;
        editor.focus_source(window, cx);
        editor
    }

    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn preview(&mut self, preview: EditorPreview, window: &mut Window, cx: &mut Context<Self>) {
        if preview == EditorPreview::Invalid {
            let source = self.draft().source.clone();
            let mut value: Value = serde_json::from_str(source.read(cx).value().as_ref()).expect("the draft was valid");
            value["language_map"].as_object_mut().expect("the draft has a language map").remove("other");
            source.update(cx, |s, cx| {
                s.replace_all(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
        }
        if preview == EditorPreview::End {
            let source = self.draft().source.clone();
            let value = source.read(cx).value().to_string();
            let row = value.lines().count().saturating_sub(1) as u32;
            let column = value.lines().last().map_or(0, |line| line.chars().count()) as u32;
            source.update(cx, |source, cx| source.set_cursor_position(Position::new(row, column), window, cx));
        }
        self.help = preview == EditorPreview::Help;
        if self.help {
            self.help_focus.focus(window, cx);
        }
        cx.emit(Changed);
        cx.notify();
    }

    /// The draft of the configuration the form shows.
    fn draft(&self) -> &Draft {
        self.drafts.get(&self.shown).expect("the configuration the form shows has a draft")
    }

    fn focus_source(&self, window: &mut Window, cx: &mut App) {
        let focus = self.draft().source.read(cx).focus_handle(cx);
        focus.focus(window, cx);
    }

    /// Show the configuration the settings select, and let go of the drafts of configurations that are gone. It runs
    /// before every draw, and after every change made here: the selection also changes elsewhere.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        self.drafts.retain(|id, _| list.items.iter().any(|configuration| configuration.id == *id));
        let id = list.selected_id();
        if self.shown != id {
            self.show(id, window, cx);
        }
    }

    /// Fill the form with the configuration `id`, starting its draft the first time.
    fn show(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        let Some(configuration) = list.items.iter().find(|configuration| configuration.id == id) else { return };
        self.start_draft(configuration, window, cx);
        self.name.update(cx, |input, cx| input.set_value(configuration.name.clone(), window, cx));
        let key = self.drafts[id].key.clone();
        self.key.update(cx, |input, cx| input.set_value(key, window, cx));
        self.shown = id.to_owned();
        self.tiles.reveal(id.to_owned());
        cx.notify();
    }

    /// The draft of `configuration`, from what is saved: its request JSON in an editor of its own, and its key.
    fn start_draft(&mut self, configuration: &CloudAsrConfiguration, window: &mut Window, cx: &mut Context<Self>) {
        if self.drafts.contains_key(&configuration.id) {
            return;
        }
        let text = serde_json::to_string_pretty(&configuration.config).expect("configurations are JSON");
        let source = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("json")
                .folding(false)
                .scroll_beyond_last_line(Some(0))
                .default_value(text.clone())
        });
        let owner = configuration.id.clone();
        cx.subscribe_in(&source, window, move |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.edited(&owner, cx);
            }
        })
        .detach();
        let key = self.st.secrets.get(&configuration.account()).unwrap_or_default();
        let error = check(&text, self.st.cfg.get().general.lang()).err();
        self.drafts.insert(
            configuration.id.clone(),
            Draft {
                source,
                saved_key: key.clone(),
                key,
                error,
                pending: false,
                revision: 0,
                test: ui::TestState::Idle,
                tested_language: None,
                styled_dark: None,
                check: None,
            },
        );
    }

    /// The name field changed: save it, and the tile follows. A blank name is not saved; the field goes back to the
    /// saved one when it loses focus.
    fn name_edited(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        self.saved(configuration_list::rename(&self.st.cfg, &self.shown, &name), cx);
        cx.notify();
    }

    fn name_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.name.read(cx).value().trim().is_empty() {
            return;
        }
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        let saved = list
            .items
            .iter()
            .find(|configuration| configuration.id == self.shown)
            .map(|configuration| configuration.name.clone());
        self.name.update(cx, |input, cx| input.set_value(saved.unwrap_or_default(), window, cx));
    }

    fn key_edited(&mut self, cx: &mut Context<Self>) {
        let key = self.key.read(cx).value().to_string();
        let id = self.shown.clone();
        if let Some(draft) = self.drafts.get_mut(&id) {
            draft.key = key;
        }
        self.edited(&id, cx);
    }

    /// The draft of `id` changed: its result and validation are out of date, and it is checked and saved once typing
    /// pauses.
    fn edited(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.drafts.get_mut(id) else { return };
        draft.test = ui::TestState::Idle;
        draft.pending = true;
        draft.error = None;
        draft.source.update(cx, |source, _| {
            if let Some(diagnostics) = source.diagnostics_mut() {
                diagnostics.clear();
            }
        });
        draft.revision += 1;
        let (owner, revision) = (id.to_owned(), draft.revision);
        draft.check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, cx| this.checked(&owner, revision, cx));
        }));
        cx.notify();
    }

    fn checked(&mut self, id: &str, revision: u64, cx: &mut Context<Self>) {
        let lang = self.st.cfg.get().general.lang();
        let Some(draft) = self.drafts.get_mut(id) else { return };
        if draft.revision != revision {
            return;
        }
        let text = draft.source.read(cx).value().to_string();
        draft.error = check(&text, lang).err();
        let marker = draft.error.clone().map(|message| (error_row(&text).saturating_sub(1) as u32, message));
        draft.source.update(cx, |source, cx| {
            if let Some(diagnostics) = source.diagnostics_mut() {
                diagnostics.clear();
                if let Some((line, message)) = marker {
                    diagnostics.push(
                        Diagnostic::new(Position::new(line, 0)..Position::new(line + 1, 0), message)
                            .with_severity(DiagnosticSeverity::Error),
                    );
                }
            }
            cx.notify();
        });
        draft.pending = false;
        if draft.error.is_none() {
            self.persist(id, cx);
        }
        cx.notify();
    }

    /// Save the draft of `id` into its configuration: its request JSON, and its key when that changed.
    fn persist(&mut self, id: &str, cx: &mut Context<Self>) {
        let lang = self.st.cfg.get().general.lang();
        let Some(draft) = self.drafts.get_mut(id) else { return };
        if draft.pending {
            return;
        }
        let text = draft.source.read(cx).value().to_string();
        let value = match check(&text, lang) {
            Ok(value) => value,
            Err(error) => {
                draft.error = Some(error);
                cx.notify();
                return;
            }
        };
        let key = KeyChange { typed: &draft.key, saved: &draft.saved_key };
        let key_changed = key.typed != key.saved;
        let saved = configuration_list::save_draft(&self.st.cfg, &*self.st.secrets, id, value, &key);
        if key_changed {
            self.st.factory.invalidate_engines();
        }
        let typed_key = draft.key.clone();
        if let Some(true) = self.saved(saved, cx)
            && let Some(draft) = self.drafts.get_mut(id)
        {
            draft.saved_key = typed_key;
        }
    }

    /// Check and save what is typed in `id` now instead of waiting out the pause, for a change that has to start from it.
    fn flush(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(draft) = self.drafts.get(id).filter(|draft| draft.pending) {
            let revision = draft.revision;
            self.checked(id, revision, cx);
        }
    }

    /// What a change to the saved configurations came to. A save that fails raises the banner above the card and
    /// leaves everything as it was; one that works takes the banner down.
    fn saved<T>(&mut self, result: anyhow::Result<T>, cx: &mut Context<Self>) -> Option<T> {
        let failed = result.is_err();
        if let Err(error) = &result {
            tracing::warn!("could not save the speech configurations: {error:#}");
        }
        if std::mem::replace(&mut self.save_failed, failed) != failed {
            cx.emit(Changed);
        }
        cx.notify();
        result.ok()
    }

    /// Whether the last save of a speech configuration failed.
    pub(in crate::settings) fn save_failed(&self) -> bool {
        self.save_failed
    }

    /// The banner was closed.
    pub(in crate::settings) fn dismiss_save_failure(&mut self, cx: &mut Context<Self>) {
        self.save_failed = false;
        cx.notify();
    }

    fn map_language_codes(&mut self, codes: [(&str, &str); 4], window: &mut Window, cx: &mut Context<Self>) {
        let source = self.draft().source.clone();
        if let Ok(mut value) = serde_json::from_str::<Value>(source.read(cx).value().as_ref())
            && let Some(map) = value["language_map"].as_object_mut()
        {
            for (k, v) in codes {
                map.insert(k.into(), v.into());
            }
            source.update(cx, |s, cx| {
                s.replace_all(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
        }
    }

    fn run_test(&mut self, cx: &mut Context<Self>) {
        let id = self.shown.clone();
        if !self.can_test(&id, cx) {
            return;
        }
        let lang = self.st.cfg.get().general.lang();
        let language = self.st.cfg.get().general.preferred_language;
        let Some(draft) = self.drafts.get_mut(&id) else { return };
        let Ok(config) = check(draft.source.read(cx).value().as_ref(), lang) else { return };
        let (key, revision) = (draft.key.clone(), draft.revision);
        draft.tested_language = Some(language);
        draft.test = ui::TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = crate::services::test_asr_draft(st, config, key, language).await;
            let _ = this.update(cx, |this, cx| {
                let same_language = this.st.cfg.get().general.preferred_language == language;
                if let Some(draft) = this.drafts.get_mut(&id)
                    && draft.revision == revision
                    && same_language
                {
                    draft.test = outcome.into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn can_test(&self, id: &str, cx: &App) -> bool {
        self.drafts.get(id).is_some_and(|draft| {
            !draft.pending
                && draft.error.is_none()
                && draft.test != ui::TestState::Running
                && (!draft.source.read(cx).value().contains("{{api_key}}") || !draft.key.trim().is_empty())
        })
    }

    fn open_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.help = true;
        self.help_focus.focus(window, cx);
        window.on_next_frame(|window, _| window.refresh());
        cx.emit(Changed);
        cx.notify();
    }

    fn back_or_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog && !self.help {
            cx.emit(Closed(true));
            return;
        }
        self.help = false;
        self.focus_source(window, cx);
        cx.emit(Changed);
        cx.notify();
    }

    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
        self.menu = false;
        self.help = false;
        // A dialog belongs to the visit that opened it.
        self.deleting = None;
    }

    /// What the shell paints over the whole window: the delete dialog, or the help.
    pub(in crate::settings) fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.deleting.is_some() {
            return ui::with_motion_scope(self.motion.handle(), || self.delete_dialog(window, cx));
        }
        if !self.help {
            return None;
        }
        Some(ui::with_motion_scope(self.motion.handle(), || self.modal(window, cx).into_any_element()))
    }
}

impl Render for AsrEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || {
            if self.dialog {
                self.modal(window, cx).into_any_element()
            } else {
                self.inline(window, cx).into_any_element()
            }
        })
    }
}

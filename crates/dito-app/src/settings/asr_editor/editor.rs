use std::sync::Arc;
use std::time::Duration;

use dito_core::provider::configured_asr::AsrProfiles;
use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity, Position};
use gpui_kit::component::input::{EditorState, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::Value;

use super::validation::{check, error_row};
use crate::state::AppState;
use crate::ui;

mod edit_panels;
mod modal;
#[cfg(feature = "screenshots")]
mod probe;
#[cfg(feature = "screenshots")]
pub use probe::EditorCommand;

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

pub struct AsrEditor {
    st: Arc<AppState>,
    source: Entity<EditorState>,
    sources: [Entity<EditorState>; 3],
    key: Entity<InputState>,
    keys: [String; 3],
    saved_keys: [String; 3],
    template_open: bool,
    help_focus: FocusHandle,
    test: ui::TestState,
    tested_language: Option<dito_core::config::PreferredLanguage>,
    drafts: Vec<String>,
    selected: usize,
    error: Option<String>,
    pending: bool,
    revision: u64,
    help: bool,
    dialog: bool,
    highlighter_dark: Option<bool>,
    help_scroll: ScrollHandle,
    motion: ui::MotionScope,
}

impl EventEmitter<Changed> for AsrEditor {}
impl EventEmitter<Closed> for AsrEditor {}

impl AsrEditor {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let p = st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let defaults = AsrProfiles::default();
        let drafts = (0..3)
            .map(|i| {
                let profile = p.profiles.get(i).or_else(|| defaults.profiles.get(i));
                serde_json::to_string_pretty(&profile.map_or(Value::Null, |profile| profile.config.clone()))
                    .expect("profiles are JSON")
            })
            .collect::<Vec<_>>();
        let selected = p.selected_index().min(2);
        crate::services::watch_config(&st, cx);
        let sources = std::array::from_fn(|i| {
            let source = cx.new(|cx| {
                EditorState::new(window, cx)
                    .language("json")
                    .folding(false)
                    .scroll_beyond_last_line(Some(0))
                    .default_value(drafts[i].clone())
            });
            cx.subscribe_in(&source, window, move |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) && this.selected == i {
                    this.changed(cx);
                }
            })
            .detach();
            source
        });
        let source = sources[selected].clone();
        let mut keyed = p.clone();
        let keys = std::array::from_fn(|i| {
            if i >= keyed.profiles.len() {
                return String::new();
            }
            keyed.select_index(i);
            st.secrets.get(&keyed.account()).unwrap_or_default()
        });
        let key = cx.new(|cx| InputState::new(window, cx).masked(true).default_value(keys[selected].clone()));
        cx.subscribe_in(&key, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.changed(cx);
            }
        })
        .detach();
        let error = check(&drafts[selected], st.cfg.get().general.lang()).err();
        Self {
            st,
            source,
            sources,
            key,
            saved_keys: keys.clone(),
            keys,
            template_open: false,
            help_focus: cx.focus_handle(),
            test: ui::TestState::Idle,
            tested_language: None,
            drafts,
            selected,
            error,
            pending: false,
            revision: 0,
            help: false,
            dialog: false,
            highlighter_dark: None,
            help_scroll: ScrollHandle::new(),
            motion: ui::MotionScope::new(),
        }
    }

    /// The onboarding wizard keeps its advanced editor within its existing single overlay.
    pub fn new_dialog(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new(st, window, cx);
        editor.dialog = true;
        editor.source.read(cx).focus_handle(cx).focus(window, cx);
        editor
    }

    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn preview(&mut self, preview: EditorPreview, window: &mut Window, cx: &mut Context<Self>) {
        if preview == EditorPreview::Invalid {
            let mut value: Value = serde_json::from_str(&self.drafts[self.selected]).expect("the draft was valid");
            value["language_map"].as_object_mut().expect("the draft has a language map").remove("other");
            self.source.update(cx, |s, cx| {
                s.replace_all(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
            self.changed(cx);
        }
        if preview == EditorPreview::End {
            let value = self.source.read(cx).value().to_string();
            let row = value.lines().count().saturating_sub(1) as u32;
            let column = value.lines().last().map_or(0, |line| line.chars().count()) as u32;
            self.source.update(cx, |source, cx| source.set_cursor_position(Position::new(row, column), window, cx));
        }
        self.help = preview == EditorPreview::Help;
        if self.help {
            self.help_focus.focus(window, cx);
        }
        cx.emit(Changed);
        cx.notify();
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
        self.keys[self.selected] = self.key.read(cx).value().to_string();
        self.test = ui::TestState::Idle;
        self.pending = true;
        self.error = None;
        self.source.update(cx, |source, _| {
            if let Some(d) = source.diagnostics_mut() {
                d.clear();
            }
        });
        self.revision += 1;
        let revision = self.revision;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, cx| this.checked(revision, cx));
        })
        .detach();
    }

    fn checked(&mut self, revision: u64, cx: &mut Context<Self>) {
        if self.revision != revision {
            return;
        }
        let draft = &self.drafts[self.selected];
        self.error = check(draft, self.st.cfg.get().general.lang()).err();
        let marker = self.error.clone().map(|message| (error_row(draft).saturating_sub(1) as u32, message));
        self.source.update(cx, |source, cx| {
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
        self.pending = false;
        if self.error.is_none() {
            self.persist(cx);
        }
        cx.notify();
    }

    fn switch(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
        self.keys[self.selected] = self.key.read(cx).value().to_string();
        self.selected = i;
        self.source = self.sources[i].clone();
        self.key.update(cx, |s, cx| s.set_value(self.keys[i].clone(), window, cx));
        self.template_open = false;
        self.source.read(cx).focus_handle(cx).focus(window, cx);
        self.changed(cx);
    }

    fn map_language_codes(&mut self, codes: [(&str, &str); 4], window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(mut value) = serde_json::from_str::<Value>(self.source.read(cx).value().as_ref())
            && let Some(map) = value["language_map"].as_object_mut()
        {
            for (k, v) in codes {
                map.insert(k.into(), v.into());
            }
            self.source.update(cx, |s, cx| {
                s.replace_all(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
            self.changed(cx);
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let value = match check(self.source.read(cx).value().as_ref(), self.st.cfg.get().general.lang()) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        let selected = self.selected;
        let mut profile = self.st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        if selected >= profile.profiles.len() {
            return;
        }
        let selection_changed = profile.selected_index() != selected;
        profile.select_index(selected);
        let account = profile.account();
        let key = self.keys[selected].clone();
        let key_changed = key != self.saved_keys[selected];
        if key_changed {
            let result =
                if key.is_empty() { self.st.secrets.delete(&account) } else { self.st.secrets.set(&account, &key) };
            self.st.factory.invalidate_engines();
            if let Err(e) = result {
                self.error = Some(e.to_string());
                return;
            }
        }
        if (selection_changed || profile.profiles.get(selected).map(|p| &p.config) != Some(&value))
            && let Err(e) = self.st.cfg.update(|cfg| {
                let profiles = cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
                if let Some(entry) = profiles.profiles.get_mut(selected) {
                    entry.config = value;
                }
                profiles.select_index(selected);
            })
        {
            if key_changed {
                let previous = &self.saved_keys[selected];
                let rollback = if previous.is_empty() {
                    self.st.secrets.delete(&account)
                } else {
                    self.st.secrets.set(&account, previous)
                };
                self.st.factory.invalidate_engines();
                if let Err(error) = rollback {
                    tracing::warn!(%error, "could not restore the previous ASR key");
                }
            }
            self.error = Some(e.to_string());
            return;
        }
        self.saved_keys[selected] = key;
    }

    fn run_test(&mut self, cx: &mut Context<Self>) {
        if !self.can_test(cx) {
            return;
        }
        let profile = match check(self.source.read(cx).value().as_ref(), self.st.cfg.get().general.lang()) {
            Ok(value) => value,
            Err(_) => return,
        };
        let key = self.key.read(cx).value().to_string();
        let revision = self.revision;
        let language = self.st.cfg.get().general.preferred_language;
        self.tested_language = Some(language);
        self.test = ui::TestState::Running;
        cx.notify();
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            let outcome = crate::services::test_asr_draft(st, profile, key, language).await;
            let _ = this.update(cx, |this, cx| {
                if this.revision == revision && this.st.cfg.get().general.preferred_language == language {
                    this.test = outcome.into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn can_test(&self, cx: &App) -> bool {
        !self.pending
            && self.error.is_none()
            && self.test != ui::TestState::Running
            && (!self.source.read(cx).value().contains("{{api_key}}") || !self.key.read(cx).value().trim().is_empty())
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
        self.source.read(cx).focus_handle(cx).focus(window, cx);
        cx.emit(Changed);
        cx.notify();
    }

    pub(in crate::settings) fn reset_motion(&mut self) {
        self.motion.reset();
        self.template_open = false;
        self.help = false;
    }

    pub(in crate::settings) fn help_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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

use std::sync::Arc;
use std::time::Duration;

use dito_core::provider::configured_asr::AsrProfiles;
use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity, Position};
use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::Value;

use super::validation::{check, error_row};
use crate::state::AppState;
use crate::ui;

mod edit_panels;
mod modal;

const ISO_CODES: [(&str, &str); 4] = [("ja", "ja"), ("en", "en"), ("zh", "zh"), ("other", "en")];
const LOCALE_CODES: [(&str, &str); 4] = [("ja", "ja-JP"), ("en", "en-US"), ("zh", "zh-CN"), ("other", "en-US")];
const CHECK_DELAY: Duration = Duration::from_millis(200);

pub struct Closed(pub bool);

/// How a frozen editor is set up for screenshots.
#[cfg(feature = "screenshots")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditorPreview {
    Valid,
    Invalid,
    Help,
}

pub struct AsrEditor {
    st: Arc<AppState>,
    source: Entity<EditorState>,
    drafts: Vec<String>,
    selected: usize,
    error: Option<String>,
    pending: bool,
    revision: u64,
    help: bool,
    highlighter_dark: Option<bool>,
    help_scroll: ScrollHandle,
    motion: ui::MotionScope,
}

impl EventEmitter<Closed> for AsrEditor {}

impl AsrEditor {
    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let p = st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let defaults = AsrProfiles::default();
        let drafts = (0..3)
            .map(|i| {
                serde_json::to_string_pretty(p.profiles.get(i).unwrap_or(&defaults.profiles[i]))
                    .expect("profiles are JSON")
            })
            .collect::<Vec<_>>();
        let selected = p.selected.min(2);
        let source = cx.new(|cx| {
            EditorState::new(window, cx).language("json").folding(false).default_value(drafts[selected].clone())
        });
        cx.subscribe_in(&source, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.changed(cx);
            }
        })
        .detach();
        let focus = source.read(cx).focus_handle(cx);
        focus.focus(window, cx);
        let error = check(&drafts[selected], st.cfg.get().general.lang()).err();
        Self {
            st,
            source,
            drafts,
            selected,
            error,
            pending: false,
            revision: 0,
            help: false,
            highlighter_dark: None,
            help_scroll: ScrollHandle::new(),
            motion: ui::MotionScope::new(),
        }
    }

    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn preview(&mut self, preview: EditorPreview, window: &mut Window, cx: &mut Context<Self>) {
        if preview == EditorPreview::Invalid {
            let mut value: Value = serde_json::from_str(&self.drafts[self.selected]).expect("the draft was valid");
            value["language_map"].as_object_mut().expect("the draft has a language map").remove("other");
            self.source.update(cx, |s, cx| {
                s.set_value(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
            self.changed(cx);
        }
        self.help = preview == EditorPreview::Help;
        cx.notify();
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
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
        cx.notify();
    }

    fn switch(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
        self.selected = i;
        self.source.update(cx, |s, cx| s.set_value(self.drafts[i].clone(), window, cx));
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
                s.set_value(serde_json::to_string_pretty(&value).expect("a JSON value serializes"), window, cx)
            });
            self.changed(cx);
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
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
        if let Err(e) = self.st.cfg.update(|cfg| {
            let profiles = cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
            while profiles.profiles.len() < 3 {
                profiles.profiles.push(AsrProfiles::default().profiles[profiles.profiles.len()].clone());
            }
            profiles.profiles[selected] = value;
            profiles.selected = selected;
        }) {
            self.error = Some(e.to_string());
            cx.notify();
            return;
        }
        cx.emit(Closed(true));
    }

    fn back_or_close(&mut self, cx: &mut Context<Self>) {
        if self.help {
            self.help = false;
            cx.notify();
        } else {
            cx.emit(Closed(false));
        }
    }
}

impl Render for AsrEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || self.modal(window, cx))
    }
}

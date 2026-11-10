//! Native interaction probes used by the isolated screenshot runner.

use std::ops::Range;

use gpui_kit::base::input::{Redo, Undo};
use gpui_kit::{EntityInputHandler as _, *};

use super::{AsrEditor, LOCALE_CODES};
use crate::ui::InputEvent;

pub enum EditorCommand {
    Undo,
    Redo,
    OpenHelp,
    CloseHelp,
    Test,
    MapLocales,
    ScrollHelp(f32),
    /// Focus the name field and select all of it.
    SelectName,
    /// Take the focus away from the name field, and send the blur an active window sends then: GPUI delivers no blur
    /// in the shot windows, which are never active.
    LeaveName,
    /// Type into the name field as a person does: the text replaces the selection, which is the whole name right
    /// after a configuration was added.
    TypeName(String),
    /// Select the whole key and type over it.
    TypeKey(String),
}

/// What the cloud speech form shows right now.
pub struct CloudAsrFormState {
    pub shown: String,
    pub name: String,
    pub name_focused: bool,
    pub name_selection: Range<usize>,
    pub key: String,
    pub menu_open: bool,
    /// The name a delete dialog asks about, while it is open.
    pub deleting: Option<String>,
}

/// What the cloud speech editor keeps for one configuration.
pub struct CloudAsrDraftState {
    pub id: String,
    pub text: String,
    pub key: String,
    pub error: Option<String>,
    pub pending: bool,
    pub test: crate::ui::TestState,
}

impl AsrEditor {
    pub(crate) fn probe_replace(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.draft().source.clone().update(cx, |source, cx| source.replace_all(value, window, cx));
    }

    pub(crate) fn probe_state(&self, cx: &App) -> (String, bool, crate::ui::TestState) {
        (self.draft().source.read(cx).value().to_string(), self.help, self.draft().test.clone())
    }

    pub(crate) fn probe_form(&self, window: &Window, cx: &App) -> CloudAsrFormState {
        let name = self.name.read(cx);
        CloudAsrFormState {
            shown: self.shown.clone(),
            name: name.value().to_string(),
            name_focused: name.focus_handle(cx).is_focused(window),
            name_selection: name.selected_range(),
            key: self.key.read(cx).value().to_string(),
            menu_open: self.tiles.menu_open(),
            deleting: self.tiles.deleting().map(|deletion| deletion.name.clone()),
        }
    }

    /// The tile row's scroll offset, the furthest it goes and the slot a dragged tile is over.
    pub(crate) fn probe_tiles(&self) -> (f32, f32, Option<usize>) {
        self.tiles.probe()
    }

    /// Every draft the editor keeps, by id.
    pub(crate) fn probe_drafts(&self, cx: &App) -> Vec<CloudAsrDraftState> {
        let mut drafts: Vec<_> = self
            .drafts
            .iter()
            .map(|(id, draft)| CloudAsrDraftState {
                id: id.clone(),
                text: draft.source.read(cx).value().to_string(),
                key: draft.key.clone(),
                error: draft.error.clone(),
                pending: draft.pending,
                test: draft.test.clone(),
            })
            .collect();
        drafts.sort_by(|a, b| a.id.cmp(&b.id));
        drafts
    }

    pub(crate) fn probe_command(&mut self, command: EditorCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            EditorCommand::Undo | EditorCommand::Redo => {
                self.focus_source(window, cx);
                let action: Box<dyn Action> = match command {
                    EditorCommand::Undo => Box::new(Undo),
                    _ => Box::new(Redo),
                };
                window.dispatch_action(action, cx);
            }
            EditorCommand::OpenHelp => self.open_help(window, cx),
            EditorCommand::CloseHelp => self.back_or_close(window, cx),
            EditorCommand::Test => self.run_test(cx),
            EditorCommand::MapLocales => self.map_language_codes(LOCALE_CODES, window, cx),
            EditorCommand::ScrollHelp(fraction) => {
                let max = self.help_scroll.max_offset().y;
                self.help_scroll.set_offset(point(px(0.), -max * fraction));
                cx.emit(super::Changed);
                cx.notify();
            }
            EditorCommand::SelectName => self.name.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            }),
            EditorCommand::LeaveName => {
                window.blur(cx);
                self.name.update(cx, |_, cx| cx.emit(InputEvent::Blur));
            }
            EditorCommand::TypeName(text) => {
                self.name.update(cx, |input, cx| input.replace_text_in_range(None, &text, window, cx));
            }
            EditorCommand::TypeKey(text) => {
                self.key.update(cx, |input, cx| {
                    input.select_all(window, cx);
                    input.replace_text_in_range(None, &text, window, cx);
                });
            }
        }
    }
}

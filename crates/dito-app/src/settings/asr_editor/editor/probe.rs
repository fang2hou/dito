//! Native interaction probes used by the isolated screenshot runner.

use gpui_kit::base::input::{Redo, Undo};
use gpui_kit::*;

use super::{AsrEditor, LOCALE_CODES};

pub enum EditorCommand {
    Undo,
    Redo,
    OpenHelp,
    CloseHelp,
    Test,
    MapLocales,
    ScrollHelp(f32),
}

impl AsrEditor {
    pub(crate) fn probe_replace(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.source.update(cx, |source, cx| source.replace_all(value, window, cx));
        self.changed(cx);
    }

    pub(crate) fn probe_state(&self, cx: &App) -> (String, bool, crate::ui::TestState) {
        (self.source.read(cx).value().to_string(), self.help, self.test.clone())
    }

    pub(crate) fn probe_command(&mut self, command: EditorCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            EditorCommand::Undo | EditorCommand::Redo => {
                self.source.read(cx).focus_handle(cx).focus(window, cx);
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
        }
    }
}

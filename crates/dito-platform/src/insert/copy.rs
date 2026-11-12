//! Reads the selection of an app that publishes no Accessibility text (Zed, terminals, custom-drawn and some Electron
//! apps) by running the app's own Copy command and reading what it put on the pasteboard. Its Cut item tells whether the
//! selection can be written over.

use std::time::{Duration, Instant};

use super::ax::AxElement;
use super::focus::frontmost_foreign_pid;
use super::keyboard::post_copy;
use super::pasteboard::{Copied, read_after_copy};

/// The menu press runs the command before it returns, so only a slow app makes this wait.
const MENU_WAIT: Duration = Duration::from_millis(200);
/// A posted key travels through the window server and the app's event loop first.
const KEY_WAIT: Duration = Duration::from_millis(300);
const AX_TIMEOUT: f32 = 0.3;
const SCAN_BUDGET: Duration = Duration::from_millis(500);
/// `AXMenuItemCmdModifiers` is a bit set: 0 is Command alone, bit 3 means "no Command".
const COMMAND_ONLY: i64 = 0;

/// What the Edit menu says about the selection: Copy to run and its state, and whether Cut is enabled.
struct EditMenu {
    copy: Option<(AxElement, bool)>,
    /// `None` when the app has no Cut item.
    cut_enabled: Option<bool>,
}

/// The app's Copy and Cut items: the ones whose shortcuts are Command-C and Command-X, whatever the menu language. The
/// Apple menu and the app menu come first and never hold them.
fn find_edit_items(pid: i32) -> EditMenu {
    let deadline = Instant::now() + SCAN_BUDGET;
    let mut found = EditMenu { copy: None, cut_enabled: None };
    let Some(menu_bar) = AxElement::application(pid).and_then(|app| app.element("AXMenuBar")) else {
        return found;
    };
    for top in menu_bar.with_timeout(AX_TIMEOUT).children().into_iter().skip(2) {
        for menu in top.with_timeout(AX_TIMEOUT).children() {
            for item in menu.with_timeout(AX_TIMEOUT).children() {
                if Instant::now() >= deadline {
                    return found;
                }
                let item = item.with_timeout(AX_TIMEOUT);
                let key = item.string("AXMenuItemCmdChar");
                let modifiers = item.integer("AXMenuItemCmdModifiers");
                match command(key.as_deref(), modifiers) {
                    Some(Command::Copy) if found.copy.is_none() => {
                        let enabled = item.is_enabled();
                        found.copy = Some((item, enabled));
                    }
                    Some(Command::Cut) if found.cut_enabled.is_none() => found.cut_enabled = Some(item.is_enabled()),
                    _ => {}
                }
            }
            // Cut and Copy share the Edit menu, so the menu that holds Copy is the last one to read.
            if found.copy.is_some() {
                return found;
            }
        }
    }
    found
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Copy,
    Cut,
}

/// The edit command a menu item runs: Command-C or Command-X alone, in either letter case.
fn command(key: Option<&str>, modifiers: Option<i64>) -> Option<Command> {
    if modifiers != Some(COMMAND_ONLY) {
        return None;
    }
    match key? {
        key if key.eq_ignore_ascii_case("c") => Some(Command::Copy),
        key if key.eq_ignore_ascii_case("x") => Some(Command::Cut),
        _ => None,
    }
}

/// How the selection is copied, and whether the text it came from can be written over.
#[derive(Debug, PartialEq, Eq)]
struct Route {
    /// The Copy item is pressed; otherwise Command-C is posted.
    menu: bool,
    editable: bool,
}

/// An enabled Copy item is pressed. A disabled one proves nothing: Zed's Markdown preview keeps Copy, Cut and Paste
/// disabled while Command-C copies the selected text, so only the keys can tell. A selection is editable when the
/// app's Cut item is enabled (Zed's editor enables it, its preview does not); an app without one is not assumed so.
fn route(copy_enabled: Option<bool>, cut_enabled: Option<bool>) -> Route {
    Route { menu: copy_enabled == Some(true), editable: cut_enabled == Some(true) }
}

/// The text of an app's selection and whether it can be written over.
#[derive(Debug)]
pub(super) struct CopiedSelection {
    pub text: String,
    pub editable: bool,
}

/// The text of the app's selection, or `None` when nothing was selected, the app copied something other than text, or
/// it never copied.
///
/// The Edit menu's Copy item is pressed through Accessibility when it is enabled: that needs no keyboard focus and no
/// key event. Otherwise (no item, or one that is disabled, see [`route`]) Command-C is posted to the app in front. The
/// clipboard is put back before returning.
pub(super) fn copy_selection(pid: i32) -> Option<CopiedSelection> {
    let started = Instant::now();
    let EditMenu { copy, cut_enabled } = find_edit_items(pid);
    let copy_enabled = copy.as_ref().map(|(_, enabled)| *enabled);
    let Route { menu, editable } = route(copy_enabled, cut_enabled);
    dito_core::trace_log!(path = "copy", ?copy_enabled, ?cut_enabled, editable, "the Edit menu was read");
    let (copied, via) = match copy {
        Some((item, _)) if menu => match read_after_copy(|| item.press(), MENU_WAIT) {
            Copied::NotStarted => (copy_by_keys(pid), "keys"),
            copied => (copied, "menu"),
        },
        _ => (copy_by_keys(pid), "keys"),
    };
    let ms = started.elapsed().as_millis();
    match copied {
        Copied::Text(text) => {
            dito_core::trace_log!(path = "copy", via, ms, editable, "the selection was copied");
            Some(CopiedSelection { text, editable })
        }
        Copied::CurrentLine => {
            dito_core::trace_log!(path = "copy", via, ms, "the app copied the current line, nothing is selected");
            None
        }
        Copied::Files => {
            dito_core::trace_log!(path = "copy", via, ms, "the app copied files, not text");
            None
        }
        Copied::Nothing => {
            dito_core::trace_log!(path = "copy", via, ms, "the copy changed nothing, nothing is selected");
            None
        }
        Copied::NotStarted => {
            dito_core::trace_log!(path = "copy", ms, "the copy could not be started");
            None
        }
    }
}

/// Keys go to the app in front, so any other app is left alone.
fn copy_by_keys(pid: i32) -> Copied {
    if frontmost_foreign_pid() != Some(pid) {
        return Copied::NotStarted;
    }
    read_after_copy(post_copy, KEY_WAIT)
}

#[cfg(test)]
mod tests {
    use super::{Command, Route, command, route};

    #[test]
    fn a_disabled_copy_item_still_copies_by_keys() {
        // Zed's Markdown preview: Copy, Cut and Paste disabled, Command-C copies the selection.
        assert_eq!(route(Some(false), Some(false)), Route { menu: false, editable: false });
    }

    #[test]
    fn an_enabled_copy_item_is_pressed() {
        assert_eq!(route(Some(true), Some(true)), Route { menu: true, editable: true });
        assert_eq!(route(Some(true), Some(false)), Route { menu: true, editable: false });
    }

    #[test]
    fn an_app_without_a_cut_item_is_not_assumed_editable() {
        assert_eq!(route(None, None), Route { menu: false, editable: false });
        assert_eq!(route(Some(true), None), Route { menu: true, editable: false });
    }

    #[test]
    fn only_command_alone_makes_copy_or_cut() {
        assert_eq!(command(Some("C"), Some(0)), Some(Command::Copy));
        assert_eq!(command(Some("x"), Some(0)), Some(Command::Cut));
        assert_eq!(command(Some("C"), Some(1)), None);
        assert_eq!(command(Some("C"), None), None);
        assert_eq!(command(Some("v"), Some(0)), None);
        assert_eq!(command(None, Some(0)), None);
    }
}

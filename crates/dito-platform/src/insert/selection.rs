//! The text selected in the app in front, captured through Accessibility and written over again later.

use std::time::{Duration, Instant};

use dito_core::config::InsertMethod;
use dito_core::controller::Delivery;

use super::ax::AxElement;
use super::copy::{CopiedSelection, copy_selection};
use super::deliver::{deliver_over_selection, terminal_clipboard};
use super::focus::{EDITABLE_ROLES, focused_foreign_element, focused_pid, frontmost_foreign_pid, is_trusted, own_pid};

const SELECTED_TEXT: &str = "AXSelectedText";
const VALUE: &str = "AXValue";
const ROLE: &str = "AXRole";
const SUBROLE: &str = "AXSubrole";
const SECURE_TEXT_FIELD: &str = "AXSecureTextField";
/// How long the source app gets to become the keyboard focus again after a dito window held it.
const REFOCUS_BUDGET: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(30);
const VERIFY_TRIES: usize = 8;
const VERIFY_POLL: Duration = Duration::from_millis(40);

/// The selection of the focused element of the app that was in front when it was captured, and the text of that element.
#[derive(Debug)]
pub struct Selection {
    /// Empty when nothing is selected and only the field's text was read.
    pub text: String,
    /// `None` for an app without Accessibility text: only the selection itself could be copied.
    pub field: Option<FieldText>,
    /// Whether the text can be written over: a text input, or a copied selection the app lets its Cut command remove.
    /// A preview, a web page or a read-only view is not.
    pub editable: bool,
    pid: i32,
    origin: Origin,
}

#[derive(Debug)]
enum Origin {
    /// Read from the focused element, which can also be written to.
    Accessibility(AxElement),
    /// Copied with the app's Copy command, so there is no element to write to. The focused window at capture time and
    /// its title tell whether the user has moved on since.
    Copy { window: Option<AxElement>, title: Option<String> },
}

/// All the text of the focused input element at capture time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldText {
    pub value: String,
    /// UTF-16 location and length of the selection in `value`, when the element reports them. The length is zero for a
    /// caret.
    pub range: Option<(usize, usize)>,
}

// SAFETY: the element is a retained, immutable CoreFoundation reference. The Accessibility API may be called from any
// thread, and nothing here mutates the reference itself.
unsafe impl Send for Selection {}
// SAFETY: see above; every use is a read-only call into the Accessibility API.
unsafe impl Sync for Selection {}

/// `None`: Accessibility is not allowed, or nothing is selected and the focused element exposes no text.
///
/// A focused element that supports text answers for itself, an empty selection included. Only when there is no such
/// element (the app draws its own text, as Zed does) is the selection copied with the app's Copy command.
pub fn capture_selection() -> Option<Selection> {
    if !is_trusted() {
        return None;
    }
    let Some(element) = focused_foreign_element() else { return capture_by_copy(None) };
    let selected = element.string(SELECTED_TEXT);
    if !exposes_text(&element, selected.is_some()) {
        return capture_by_copy(Some(element));
    }
    let text = selected.unwrap_or_default();
    let field = read_field(&element);
    let editable = is_editable(&element);
    if text.is_empty() && field.is_none() {
        dito_core::trace_log!(path = "accessibility", "the focused element has no selection and no text");
        return None;
    }
    dito_core::trace_log!(
        path = "accessibility",
        selected = !text.is_empty(),
        field = field.is_some(),
        editable,
        "selection read"
    );
    Some(Selection { text, field, editable, pid: element.pid(), origin: Origin::Accessibility(element) })
}

/// Whether the element is a text element: it answers `AXSelectedText` or `AXSelectedTextRange`, or it is an input.
fn exposes_text(element: &AxElement, answers_selected_text: bool) -> bool {
    answers_selected_text
        || element.selection().is_some()
        || element.string(ROLE).is_some_and(|role| EDITABLE_ROLES.contains(&role.as_str()))
}

/// Whether the element takes text: an input, an element whose selection or value can be set, or a descendant of an
/// editable node (WebKit and Chromium focus those inside a contenteditable).
fn is_editable(element: &AxElement) -> bool {
    element.string(ROLE).is_some_and(|role| EDITABLE_ROLES.contains(&role.as_str()))
        || element.is_settable(SELECTED_TEXT)
        || element.is_settable(VALUE)
        || element.element("AXEditableAncestor").is_some_and(|ancestor| ancestor.pid() == element.pid())
}

fn capture_by_copy(element: Option<AxElement>) -> Option<Selection> {
    let pid = element.as_ref().map(AxElement::pid).or_else(frontmost_foreign_pid).filter(|pid| *pid != own_pid())?;
    dito_core::trace_log!(path = "copy", pid, "the focused element has no text, copying the selection");
    let CopiedSelection { text, editable } = copy_selection(pid)?;
    let window = AxElement::application(pid).and_then(|app| app.element("AXFocusedWindow"));
    let title = window.as_ref().and_then(|window| window.string("AXTitle"));
    Some(Selection { text, field: None, editable, pid, origin: Origin::Copy { window, title } })
}

/// The text of a text input, for context only. A password field is never read.
fn read_field(element: &AxElement) -> Option<FieldText> {
    if element.string(SUBROLE).as_deref() == Some(SECURE_TEXT_FIELD) {
        return None;
    }
    let selection = element.selection();
    let is_input =
        selection.is_some() || element.string(ROLE).is_some_and(|role| EDITABLE_ROLES.contains(&role.as_str()));
    let value = element.string(VALUE).filter(|value| is_input && !value.trim().is_empty())?;
    Some(FieldText { value, range: selection.and_then(|selection| selection.range()) })
}

impl Selection {
    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// Hands keyboard focus back to the source app without activating dito, then writes `text` over the selection. The
    /// selection must still be what was captured; otherwise the user moved on and the text goes to the clipboard
    /// instead of over something else. Blocking.
    ///
    /// Setting `AXSelectedText` is preferred because it needs no keyboard focus and touches no clipboard. Apps that
    /// accept the call and ignore it (Chromium) are detected by reading the element back, and then the usual insertion
    /// path types or pastes over the still-selected text.
    ///
    /// A selection that was copied (an app without Accessibility text) has no element to write to, so it goes straight
    /// to the insertion path, always by paste whatever `method` says. That path cannot read such an app back: the
    /// outcome is accepted unverified, and nothing is retried.
    pub fn replace(&self, text: &str, method: InsertMethod) -> Delivery {
        let focused = self.regain_focus();
        match &self.origin {
            Origin::Accessibility(element) => {
                if !self.still_selected(element) {
                    tracing::warn!("selection: the selection changed since it was captured; text left on clipboard");
                    return terminal_clipboard(text);
                }
                if self.write_through_ax(element, text) {
                    return Delivery::Inserted;
                }
            }
            Origin::Copy { window, title } => {
                if !same_window(self.pid, window.as_ref(), title.as_deref()) {
                    tracing::warn!("selection: the window changed since it was copied; text left on clipboard");
                    return terminal_clipboard(text);
                }
            }
        }
        if !focused {
            tracing::warn!("selection: keyboard focus did not return to the source app; text left on clipboard");
            return terminal_clipboard(text);
        }
        let method = match self.origin {
            Origin::Accessibility(_) => method,
            // Such an app is a custom-drawn editor whose keys the app interprets. Zed's vim mode turns typed letters
            // over a selection into commands, while Paste replaces the selection in every mode.
            Origin::Copy { .. } => InsertMethod::Paste,
        };
        deliver_over_selection(self.pid, text, method)
    }

    /// Polls until the keyboard focus is in the source app, nudging its window once. `true` also when the focus cannot
    /// be read at all (apps with an unbuilt tree), which the later steps treat as unknown rather than wrong.
    fn regain_focus(&self) -> bool {
        let deadline = Instant::now() + REFOCUS_BUDGET;
        let mut nudged = false;
        loop {
            match focused_pid() {
                Some(pid) if pid != self.pid => {
                    if !nudged {
                        nudged = true;
                        self.raise_window();
                    }
                }
                _ => return true,
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(POLL);
        }
    }

    fn raise_window(&self) {
        match &self.origin {
            Origin::Accessibility(element) => {
                if let Some(window) = element.element("AXWindow") {
                    window.set_true("AXMain");
                }
                element.set_true("AXFocused");
            }
            Origin::Copy { window, .. } => {
                if let Some(window) = window {
                    window.set_true("AXMain");
                }
            }
        }
    }

    fn still_selected(&self, element: &AxElement) -> bool {
        let deadline = Instant::now() + REFOCUS_BUDGET;
        loop {
            if element.string(SELECTED_TEXT).as_deref() == Some(self.text.as_str()) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(POLL);
        }
    }

    /// `true` when the text has landed or may have: nothing is typed over a selection that already changed.
    fn write_through_ax(&self, element: &AxElement, text: &str) -> bool {
        if !element.is_settable(SELECTED_TEXT) {
            return false;
        }
        let value_before = element.string(VALUE);
        if !element.set_string(SELECTED_TEXT, text) {
            return false;
        }
        let mut selection = None;
        for _ in 0..VERIFY_TRIES {
            std::thread::sleep(VERIFY_POLL);
            selection = element.string(SELECTED_TEXT);
            let value_after = element.string(VALUE);
            if took_effect(&self.text, text, value_before.as_deref(), value_after.as_deref(), selection.as_deref()) {
                return true;
            }
        }
        if selection.as_deref() == Some(self.text.as_str()) {
            tracing::info!("selection: setting AXSelectedText had no effect; typing over the selection instead");
            return false;
        }
        tracing::warn!("selection: could not confirm the replacement, but the selection moved; not typing it again");
        true
    }
}

/// Whether the app's focused window is still the one the selection was copied in, with the same title (a tab or file
/// switch shows there). A copied selection cannot be read back, so this is the only sign that the user moved on. A
/// window that could not be read at capture time cannot be compared.
fn same_window(pid: i32, window: Option<&AxElement>, title: Option<&str>) -> bool {
    let Some(window) = window else { return true };
    let Some(current) = AxElement::application(pid).and_then(|app| app.element("AXFocusedWindow")) else {
        return false;
    };
    current == *window && current.string("AXTitle").as_deref() == title
}

/// Whether reading the element back shows that `new` replaced `old`. The field's value is the better witness; without
/// one (rich text views that publish none) the selection leaving `old` is all that is left to see.
fn took_effect(
    old: &str,
    new: &str,
    value_before: Option<&str>,
    value_after: Option<&str>,
    selection: Option<&str>,
) -> bool {
    if old == new {
        return true;
    }
    match (value_before, value_after) {
        (Some(before), Some(after)) => {
            let units = |s: &str| s.encode_utf16().count();
            let expected_len = (units(before) + units(new)).checked_sub(units(old));
            after != before && (after.contains(new) || Some(units(after)) == expected_len)
        }
        _ => selection.is_some_and(|selection| selection != old),
    }
}

#[cfg(test)]
mod tests {
    use super::took_effect;

    #[test]
    fn the_value_changing_to_contain_the_text_confirms() {
        assert!(took_effect("old", "new text", Some("a old b"), Some("a new text b"), Some("old")));
    }

    #[test]
    fn an_untouched_value_refutes_even_if_the_call_succeeded() {
        assert!(!took_effect("old", "new", Some("a old b"), Some("a old b"), Some("old")));
    }

    #[test]
    fn a_rewritten_text_is_confirmed_by_length() {
        // Smart quotes: the same number of units, different characters.
        assert!(took_effect("old", "\"q\"", Some("a old b"), Some("a \u{201c}q\u{201d} b"), None));
    }

    #[test]
    fn without_a_value_the_selection_leaving_the_old_text_confirms() {
        assert!(took_effect("old", "new", None, None, Some("")));
        assert!(!took_effect("old", "new", None, None, Some("old")));
        assert!(!took_effect("old", "new", None, None, None));
    }

    #[test]
    fn replacing_with_the_same_text_needs_no_witness() {
        assert!(took_effect("same", "same", Some("same"), Some("same"), Some("same")));
    }
}

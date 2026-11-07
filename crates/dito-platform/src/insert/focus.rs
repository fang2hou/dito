use std::time::{Duration, Instant};

use objc2_app_kit::NSWorkspace;

use super::ax::AxElement;
use crate::permissions::{self, Permission, Status};

const FOCUS_BUDGET: Duration = Duration::from_millis(300);
const FOCUS_POLL: Duration = Duration::from_millis(30);
/// How long delivery may wait for a lazily built accessibility tree (Electron/Chromium apps
/// can take over a second). Normally the tree is warmed while the user speaks; this budget is
/// the backstop for very short utterances.
const AX_TREE_BUDGET: Duration = Duration::from_millis(1500);
const EDITABLE_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];
const MANUAL_ACCESSIBILITY: &str = "AXManualAccessibility";
/// How many elements the search below the focused window may visit, and how deep it goes.
const DESCENDANT_BUDGET: usize = 32;
const DESCENDANT_TIME: Duration = Duration::from_millis(80);
const DESCENDANT_DEPTH: usize = 4;

/// One look at whatever currently has keyboard focus.
#[derive(Debug, Clone)]
pub(super) struct FocusTarget {
    pub pid: i32,
    pub role: String,
    pub title: String,
    pub editable: bool,
    /// The focused element is the app's key window itself (`AXFocused` = true on an `AXWindow`).
    pub key_window: bool,
    /// UTF-16 location of the selection inside the focused field, if it exposes one.
    pub cursor: Option<usize>,
    pub element: AxElement,
}

impl FocusTarget {
    /// Identity for the stability check: the same field in the same app. Value and selection are
    /// excluded on purpose, because they change as text arrives.
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        self.pid == other.pid && self.element == other.element
    }

    /// GPU-rendered apps (gpui: Zed, Herdr, ...) publish no text nodes, so the focused element is
    /// the key window and nothing deeper. A key window of another process is exactly where HID
    /// keyboard events go, so live insertion is attempted even though it cannot be verified.
    pub fn foreign_key_window(&self) -> bool {
        self.key_window && self.pid != own_pid()
    }

    fn deliverable(&self) -> bool {
        self.editable || self.foreign_key_window()
    }
}

pub(super) fn own_pid() -> i32 {
    std::process::id() as i32
}

fn is_trusted() -> bool {
    permissions::status(Permission::Accessibility) == Status::Granted
}

/// The frontmost app's process id, unless that app is dito itself.
fn frontmost_foreign_pid() -> Option<i32> {
    let pid = NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier();
    (pid != own_pid()).then_some(pid)
}

fn describe(element: AxElement) -> FocusTarget {
    let role = element.string("AXRole").unwrap_or_default();
    let title = element.string("AXTitle").unwrap_or_default();
    let cursor = element.selection().and_then(|selection| selection.location());
    let editable = EDITABLE_ROLES.contains(&role.as_str()) || cursor.is_some();
    let key_window = role == "AXWindow" && element.flag("AXFocused");
    FocusTarget { pid: element.pid(), role, title, editable, key_window, cursor, element }
}

fn describe_focused(element: AxElement) -> FocusTarget {
    let target = describe(element);
    if !target.editable
        && let Some(ancestor) = target.element.element("AXEditableAncestor")
        && ancestor.pid() == target.pid
    {
        // WebKit can focus a descendant of a contenteditable node. Its editor may expose text
        // markers instead of integer offsets. Treat that as editable but keep verification unknown.
        let mut editor = describe(ancestor);
        editor.editable |= editor.element.attribute("AXSelectedTextMarkerRange").is_some();
        if editor.editable {
            return editor;
        }
    }
    target
}

pub(super) fn read_focus() -> Option<FocusTarget> {
    if !is_trusted() {
        return None;
    }
    let system = AxElement::system_wide()?;
    let global = system.element("AXFocusedUIElement").map(describe_focused);
    if global.as_ref().is_some_and(|target| target.editable || target.pid == own_pid()) {
        return global;
    }
    let app = system.element("AXFocusedApplication").or_else(|| AxElement::application(frontmost_foreign_pid()?));
    let app_focus = app.as_ref().and_then(|app| app.element("AXFocusedUIElement")).map(describe_focused);
    if app_focus.as_ref().is_some_and(|target| target.editable) {
        return app_focus;
    }
    let window = app.and_then(|app| app.element("AXFocusedWindow"));
    if let Some(window) = window {
        let mut budget = DESCENDANT_BUDGET;
        if let Some(found) =
            editable_descendant(&window, DESCENDANT_DEPTH, &mut budget, Instant::now() + DESCENDANT_TIME)
        {
            return Some(found);
        }
        // A foreign focused window can receive HID input even when it has no accessibility tree.
        return global.or_else(|| {
            let mut target = describe(window);
            target.key_window = target.role == "AXWindow";
            Some(target)
        });
    }
    app_focus.or(global)
}

/// Only inspect descendants the app explicitly marks focused. Another editable field in the
/// same window is not evidence of where synthetic keys would go.
fn editable_descendant(
    element: &AxElement,
    depth: usize,
    budget: &mut usize,
    deadline: Instant,
) -> Option<FocusTarget> {
    if depth == 0 || *budget == 0 || Instant::now() >= deadline {
        return None;
    }
    for child in element.children() {
        if *budget == 0 || Instant::now() >= deadline {
            return None;
        }
        *budget -= 1;
        if child.flag("AXFocused") {
            let target = describe_focused(child.clone());
            if target.editable {
                return Some(target);
            }
        }
        if let Some(found) = editable_descendant(&child, depth - 1, budget, deadline) {
            return Some(found);
        }
    }
    None
}

/// Chromium/Electron apps (VS Code forks and friends) keep their accessibility tree unbuilt
/// until a client asks: with it off, the system-wide focused element reads as nil and delivery
/// would fall back to the clipboard. Flipping the app's `AXManualAccessibility` builds the
/// tree, the same thing macOS does when VoiceOver connects, after which the real focused
/// text area resolves as a normal editable element. No-op for apps without the attribute.
fn enable_chromium_accessibility() -> Option<i32> {
    let pid = frontmost_foreign_pid()?;
    let app = AxElement::application(pid)?;
    let already_enabled = app.flag(MANUAL_ACCESSIBILITY);
    let supported = || app.attribute_names().is_some_and(|names| names.iter().any(|n| n == MANUAL_ACCESSIBILITY));
    (already_enabled || (supported() && app.set_true(MANUAL_ACCESSIBILITY))).then_some(pid)
}

/// Kick the target app's accessibility machinery the moment recording starts, so a lazily
/// built tree (Electron/Chromium: GitHub Desktop, Feishu, ...) is ready by delivery time. The
/// user speaks for seconds; the first attempt at delivery would otherwise pay the tree-build
/// latency and often lose the race.
pub fn warm_focus() {
    std::thread::spawn(|| {
        if !is_trusted() || read_focus().is_some_and(|target| target.editable) {
            return;
        }
        if let Some(pid) = enable_chromium_accessibility() {
            tracing::info!(
                "insert: frontmost app (pid {pid}) keeps its accessibility tree off; warming it during recording"
            );
        }
    });
}

/// Does the focused UI element accept typed text? Either a readable text field, or a key
/// window of another app that publishes no text tree but receives HID keys. (Needs the
/// Accessibility permission.)
pub fn has_text_target() -> bool {
    read_focus().is_some_and(|t| t.deliverable())
}

/// Wait until the focused element stops changing AND is a deliverable target: editable, or a
/// key window that takes HID keys. Focus settles asynchronously after HUD interactions, app
/// switches and field clicks, and Chromium-family editors (Typora's fork, Electron) report a
/// generic container first while the real editable node materializes a moment later, so a
/// stable-but-generic read is not final and polling continues. On timeout, report focus only
/// if the same element was observed twice.
pub(super) fn wait_for_stable_focus() -> Option<FocusTarget> {
    if !is_trusted() {
        return None;
    }
    let started = Instant::now();
    let mut deadline = started + FOCUS_BUDGET;
    let mut prev = read_focus();
    if !prev.as_ref().is_some_and(|target| target.editable)
        && let Some(pid) = enable_chromium_accessibility()
    {
        tracing::info!(
            "insert: frontmost app (pid {pid}) keeps its accessibility tree off; enabled it and waiting for the tree"
        );
        // The tree is built asynchronously; give Chromium room before falling back.
        deadline = Instant::now() + AX_TREE_BUDGET;
        prev = read_focus();
    }
    // A generic container may still resolve into an editable node; give the lazy tree the same
    // room before classifying it as undeliverable.
    if prev.as_ref().is_some_and(|t| !t.deliverable()) {
        deadline = Instant::now() + AX_TREE_BUDGET;
    }
    loop {
        std::thread::sleep(FOCUS_POLL);
        let cur = read_focus();
        let stable = matches!((&prev, &cur), (Some(a), Some(b)) if a.same_identity(b));
        let ready = cur.as_ref().is_some_and(|target| {
            target.editable || (target.foreign_key_window() && Instant::now() >= started + FOCUS_BUDGET)
        });
        prev = cur;
        if stable && ready {
            return prev;
        }
        if Instant::now() >= deadline {
            if !stable {
                tracing::warn!("insert: focus could not be established after {:?}", started.elapsed());
            }
            return if stable { prev } else { None };
        }
    }
}

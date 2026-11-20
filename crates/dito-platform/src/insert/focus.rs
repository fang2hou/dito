use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
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
pub(super) const EDITABLE_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];
const MANUAL_ACCESSIBILITY: &str = "AXManualAccessibility";
const DESCENDANT_BUDGET: usize = 32;
const DESCENDANT_TIME: Duration = Duration::from_millis(80);
const DESCENDANT_DEPTH: usize = 4;

static WARM_FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);
static WARMED_FOCUS: Mutex<Option<FocusReading<FocusTarget>>> = Mutex::new(None);

#[derive(Debug)]
struct FocusReading<T> {
    target: T,
    observed_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FocusReadiness {
    Editable,
    ForeignKeyWindow,
    NotDeliverable,
}

trait StableFocus {
    fn same_identity(&self, other: &Self) -> bool;
    fn readiness(&self) -> FocusReadiness;
}

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
    /// Value and selection are excluded on purpose, because they change as text arrives.
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

impl StableFocus for FocusTarget {
    fn same_identity(&self, other: &Self) -> bool {
        FocusTarget::same_identity(self, other)
    }

    fn readiness(&self) -> FocusReadiness {
        if self.editable {
            FocusReadiness::Editable
        } else if self.foreign_key_window() {
            FocusReadiness::ForeignKeyWindow
        } else {
            FocusReadiness::NotDeliverable
        }
    }
}

fn should_accept_warmed_focus<T: StableFocus>(
    kept: Option<&FocusReading<T>>,
    current: Option<&T>,
    elapsed: Duration,
) -> bool {
    let (Some(kept), Some(current)) = (kept, current) else {
        return false;
    };
    if !kept.target.same_identity(current) {
        return false;
    }
    match (kept.target.readiness(), current.readiness()) {
        (FocusReadiness::Editable, FocusReadiness::Editable) => true,
        (FocusReadiness::ForeignKeyWindow, FocusReadiness::ForeignKeyWindow) => elapsed >= FOCUS_BUDGET,
        _ => false,
    }
}

fn next_warm_focus_generation() -> u64 {
    WARM_FOCUS_GENERATION.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

fn replace_warmed_focus(generation: u64, reading: Option<FocusReading<FocusTarget>>) {
    let mut slot = WARMED_FOCUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if WARM_FOCUS_GENERATION.load(Ordering::Relaxed) == generation {
        *slot = reading;
    }
}

fn take_warmed_focus() -> Option<FocusReading<FocusTarget>> {
    next_warm_focus_generation();
    WARMED_FOCUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
}

pub(super) fn own_pid() -> i32 {
    std::process::id() as i32
}

pub(super) fn is_trusted() -> bool {
    permissions::status(Permission::Accessibility) == Status::Granted
}

/// The frontmost app's process id, unless that app is dito itself.
pub(super) fn frontmost_foreign_pid() -> Option<i32> {
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

/// The focused element of the app in front. When keyboard focus sits on one of dito's own windows the frontmost foreign
/// app is asked instead, so the source of a selection is found even while a dito panel is key.
fn read_focused_foreign_element() -> Option<AxElement> {
    let system = AxElement::system_wide()?;
    let global = system.element("AXFocusedUIElement").filter(|element| element.pid() != own_pid());
    global.or_else(|| {
        let app = system
            .element("AXFocusedApplication")
            .filter(|app| app.pid() != own_pid())
            .or_else(|| AxElement::application(frontmost_foreign_pid()?))?;
        app.element("AXFocusedUIElement")
    })
}

/// Chromium-family apps answer nothing until their accessibility tree is switched on, so one empty read triggers that
/// and waits one focus budget for the tree.
pub(super) fn focused_foreign_element() -> Option<AxElement> {
    if !is_trusted() {
        return None;
    }
    if let Some(element) = read_focused_foreign_element() {
        return Some(element.with_timeout(1.0));
    }
    enable_chromium_accessibility()?;
    let deadline = Instant::now() + FOCUS_BUDGET;
    while Instant::now() < deadline {
        std::thread::sleep(FOCUS_POLL);
        if let Some(element) = read_focused_foreign_element() {
            return Some(element.with_timeout(1.0));
        }
    }
    None
}

/// The process that owns the focused element, dito's own included. `None` when nothing can be read.
pub(super) fn focused_pid() -> Option<i32> {
    if !is_trusted() {
        return None;
    }
    let system = AxElement::system_wide()?;
    let focused = system.element("AXFocusedUIElement").or_else(|| {
        let app =
            system.element("AXFocusedApplication").or_else(|| AxElement::application(frontmost_foreign_pid()?))?;
        app.element("AXFocusedUIElement")
    });
    focused.map(|element| element.pid())
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

/// Chromium/Electron apps keep their accessibility tree unbuilt until a client asks: with it off, the system-wide
/// focused element reads as nil and delivery would fall back to the clipboard. Flipping `AXManualAccessibility`
/// builds the tree, as macOS does when VoiceOver connects. No-op for apps without the attribute.
fn enable_chromium_accessibility() -> Option<i32> {
    let pid = frontmost_foreign_pid()?;
    let app = AxElement::application(pid)?;
    let already_enabled = app.flag(MANUAL_ACCESSIBILITY);
    let supported = || app.attribute_names().is_some_and(|names| names.iter().any(|n| n == MANUAL_ACCESSIBILITY));
    (already_enabled || (supported() && app.set_true(MANUAL_ACCESSIBILITY))).then_some(pid)
}

/// Called the moment recording starts: the user speaks for seconds, while the first delivery attempt would otherwise
/// pay the tree-build latency of a lazily built tree and often lose the race.
pub fn warm_focus() {
    let generation = next_warm_focus_generation();
    *WARMED_FOCUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    std::thread::spawn(move || {
        if !is_trusted() {
            return;
        }
        let target = read_focus();
        let observed_at = Instant::now();
        if target.as_ref().is_some_and(|target| target.editable) {
            replace_warmed_focus(generation, target.map(|target| FocusReading { target, observed_at }));
            return;
        }
        if let Some(pid) = enable_chromium_accessibility() {
            tracing::info!(
                "insert: frontmost app (pid {pid}) keeps its accessibility tree off; warming it during recording"
            );
            replace_warmed_focus(generation, None);
            return;
        }
        replace_warmed_focus(generation, target.map(|target| FocusReading { target, observed_at }));
    });
}

/// Also true for a key window of another app that publishes no text tree but receives HID keys. Needs the
/// Accessibility permission.
pub fn has_text_target() -> bool {
    read_focus().is_some_and(|t| t.deliverable())
}

/// Focus settles asynchronously after HUD interactions, app switches and field clicks, and Chromium-family editors
/// report a generic container first while the real editable node materializes a moment later, so a stable but generic
/// read is not final and polling continues. On timeout, report focus only if the same element was observed twice.
pub(super) fn wait_for_stable_focus() -> Option<FocusTarget> {
    let kept = take_warmed_focus();
    if !is_trusted() {
        return None;
    }
    let started = Instant::now();
    let mut deadline = started + FOCUS_BUDGET;
    let mut prev = read_focus();
    if let Some(kept) = kept.as_ref()
        && should_accept_warmed_focus(Some(kept), prev.as_ref(), kept.observed_at.elapsed())
    {
        // A recording-start read plus this unchanged identity are the two stability readings.
        return prev;
    }
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{FOCUS_BUDGET, FocusReadiness, FocusReading, StableFocus, should_accept_warmed_focus};

    #[derive(Clone, Copy)]
    struct FakeTarget {
        identity: i32,
        readiness: FocusReadiness,
    }

    impl StableFocus for FakeTarget {
        fn same_identity(&self, other: &Self) -> bool {
            self.identity == other.identity
        }

        fn readiness(&self) -> FocusReadiness {
            self.readiness
        }
    }

    fn accepts(kept: Option<&FocusReading<FakeTarget>>, current: Option<&FakeTarget>, elapsed: Duration) -> bool {
        should_accept_warmed_focus(kept, current, elapsed)
    }

    fn reading(identity: i32, readiness: FocusReadiness) -> FocusReading<FakeTarget> {
        FocusReading { target: FakeTarget { identity, readiness }, observed_at: Instant::now() }
    }

    #[test]
    fn accepts_the_same_editable_focus_immediately() {
        let kept = reading(1, FocusReadiness::Editable);
        assert!(accepts(Some(&kept), Some(&kept.target), FOCUS_BUDGET / 100));
    }

    #[test]
    fn accepts_the_same_old_key_window_immediately() {
        let kept = reading(1, FocusReadiness::ForeignKeyWindow);
        assert!(accepts(Some(&kept), Some(&kept.target), FOCUS_BUDGET));
    }

    #[test]
    fn a_recent_key_window_still_waits() {
        let kept = reading(1, FocusReadiness::ForeignKeyWindow);
        let elapsed = FOCUS_BUDGET - Duration::from_millis(1);
        assert!(!accepts(Some(&kept), Some(&kept.target), elapsed));
    }

    #[test]
    fn a_different_identity_still_waits() {
        let kept = reading(1, FocusReadiness::Editable);
        let current = FakeTarget { identity: 2, readiness: FocusReadiness::Editable };
        assert!(!accepts(Some(&kept), Some(&current), FOCUS_BUDGET));
    }

    #[test]
    fn no_kept_reading_still_waits() {
        let current = FakeTarget { identity: 1, readiness: FocusReadiness::Editable };
        assert!(!accepts(None, Some(&current), FOCUS_BUDGET));
    }

    #[test]
    fn a_not_deliverable_target_still_waits() {
        let kept = reading(1, FocusReadiness::NotDeliverable);
        assert!(!accepts(Some(&kept), Some(&kept.target), FOCUS_BUDGET));
    }
}

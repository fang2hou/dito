//! Delivering text into the focused app, or to the clipboard when nothing editable has focus.
//!
//! Delivery waits for the keyboard focus to settle, then drives the type → paste → clipboard
//! fallback from [`dito_core::insert`], verifying each live attempt through the target field's
//! selection before accepting it. Every failed step is visible in `tracing`.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dito_core::config::InsertMethod;
use dito_core::controller::{Delivery, Sink};
use dito_core::insert::{Flow, Next, Outcome, Step, TYPE_CHUNK_GAP_MS, TYPE_CHUNK_UNITS, Utf16Chunks, outcome};

type AXUIElementRef = *const c_void;

/// `kAXValueTypeCFRange` from AXValue.h: the wrapper type of `AXSelectedTextRange` values.
const AX_VALUE_TYPE_CF_RANGE: u32 = 4;

/// Poll cadence for the focus-stability and insertion-verification waits.
const FOCUS_BUDGET: Duration = Duration::from_millis(300);
const FOCUS_POLL: Duration = Duration::from_millis(30);
/// How long delivery may wait for a lazily built accessibility tree (Electron/Chromium apps
/// can take over a second). Normally the tree is warmed while the user speaks; this budget is
/// the backstop for very short utterances.
const AX_TREE_BUDGET: Duration = Duration::from_millis(1500);
const VERIFY_TRIES: usize = 8;
const VERIFY_POLL: Duration = Duration::from_millis(40);
/// The user's clipboard goes back no sooner than this after a paste, and only once the insert
/// has been confirmed (apps read the pasteboard asynchronously, when they get to the Cmd+V).
const RESTORE_FLOOR: Duration = Duration::from_millis(500);
/// When verification cannot confirm the insert (e.g. secure fields), give slow apps longer.
const RESTORE_FLOOR_UNVERIFIED: Duration = Duration::from_millis(1000);

#[repr(C)]
struct CFRange {
    location: isize,
    /// Layout only: AXValueGetValue writes it, dito never reads it.
    _length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut *const c_void)
    -> i32;
    fn AXUIElementCopyAttributeNames(element: AXUIElementRef, names: *mut CFArrayRef) -> i32;
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXValueGetValue(value: AXUIElementRef, the_type: u32, value: *mut c_void) -> bool;
    fn AXIsProcessTrusted() -> bool;
}

const AX_SUCCESS: i32 = 0;
const EDITABLE_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

fn ax_attr(element: AXUIElementRef, name: &str) -> Option<CFType> {
    let attr = CFString::new(name);
    let mut out: *const c_void = std::ptr::null();
    // SAFETY: `element` is a live AX element, `attr` a valid CFString, and `out` a valid out-pointer.
    let err = unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut out) };
    // SAFETY: on success the call returns a +1 retained CF object, which `wrap_under_create_rule` adopts.
    (err == AX_SUCCESS && !out.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(out) })
}

/// One look at whatever currently has keyboard focus.
#[derive(Debug, Clone)]
struct FocusTarget {
    pid: i32,
    role: String,
    title: String,
    editable: bool,
    /// The focused element is the app's key window itself (`AXFocused` = true on an `AXWindow`).
    key_window: bool,
    /// UTF-16 location of the selection inside the focused field, if it exposes one.
    cursor: Option<usize>,
}

impl FocusTarget {
    /// Identity for the stability check: the same field in the same app. Value and selection are
    /// excluded on purpose — they change as text arrives.
    fn same_identity(&self, other: &Self) -> bool {
        self.pid == other.pid && self.role == other.role && self.title == other.title
    }

    /// GPU-rendered apps (gpui: Zed, Herdr, …) publish no text nodes, so the focused element is
    /// the key window and nothing deeper. A key window of another process is exactly where HID
    /// keyboard events go, so live insertion is attempted even though it cannot be verified.
    fn foreign_key_window(&self) -> bool {
        self.key_window && self.pid != std::process::id() as i32
    }
}

fn is_trusted() -> bool {
    // SAFETY: plain C call.
    unsafe { AXIsProcessTrusted() }
}

fn focused_element() -> Option<CFType> {
    // SAFETY: returns a +1 retained system-wide element, released below.
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        return None;
    }
    let focused = ax_attr(system, "AXFocusedUIElement");
    // SAFETY: `system` was created above and is not used afterwards.
    unsafe { CFRelease(system) };
    focused
}

fn range_location(value: AXUIElementRef) -> Option<usize> {
    let mut range = CFRange { location: 0, _length: 0 };
    // SAFETY: `value` is the AXValue CF object returned for AXSelectedTextRange, and `range` is a
    // valid CFRange out-buffer; the call only writes through it when the type matches.
    let ok = unsafe { AXValueGetValue(value, AX_VALUE_TYPE_CF_RANGE, &mut range as *mut CFRange as *mut c_void) };
    ok.then(|| range.location.max(0) as usize)
}

fn read_focus() -> Option<FocusTarget> {
    if !is_trusted() {
        return None;
    }
    focused_element()
        .and_then(|f| target_from(f.as_CFTypeRef()))
        // Chromium builds that reject every enable switch (Feishu/Lark) never resolve the
        // system-wide focused element, yet still answer window-level queries. Resolve through
        // the frontmost app's focused window instead, digging for a real editable descendant
        // first (Typora-style lazy web trees expose one a level or two down).
        .or_else(window_focus_target)
}

/// Describe any live AX element as a [`FocusTarget`].
fn target_from(el: AXUIElementRef) -> Option<FocusTarget> {
    let role = ax_attr(el, "AXRole").and_then(|r| r.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default();
    let title =
        ax_attr(el, "AXTitle").and_then(|r| r.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default();
    let range = ax_attr(el, "AXSelectedTextRange");
    let cursor = range.as_ref().and_then(|r| range_location(r.as_CFTypeRef()));
    // Web and custom editors expose a selection range even when the role is generic.
    let editable = EDITABLE_ROLES.contains(&role.as_str()) || range.is_some();
    let key_window = role == "AXWindow" && is_true(ax_attr(el, "AXFocused"));
    let mut pid = 0;
    // SAFETY: `el` is a live AX element and `pid` a valid out-pointer.
    unsafe { AXUIElementGetPid(el, &mut pid) };
    Some(FocusTarget { pid, role, title, editable, key_window, cursor })
}

/// The frontmost app's focused window as a fallback focus target, or an editable element
/// found inside it. `None` when the frontmost app is dito itself or nothing is readable.
fn window_focus_target() -> Option<FocusTarget> {
    use objc2_app_kit::NSWorkspace;
    let pid = NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier();
    if pid == std::process::id() as i32 {
        return None;
    }
    // SAFETY: returns a +1 retained app element, released before returning.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    let win = ax_attr(app, "AXFocusedWindow");
    // SAFETY: `app` was created above and is not used afterwards.
    unsafe { CFRelease(app) };
    let win = win?;
    let el = win.as_CFTypeRef();
    let mut budget = 60usize;
    if let Some(found) = editable_descendant(el, 4, &mut budget) {
        return Some(found);
    }
    let mut target = target_from(el)?;
    // A focused window of a foreign app is where HID keys go even without deeper knowledge.
    if target.role == "AXWindow" {
        target.key_window = true;
    }
    Some(target)
}

/// Bounded search for an element that takes text (editable role or selection range).
fn editable_descendant(el: AXUIElementRef, depth: usize, budget: &mut usize) -> Option<FocusTarget> {
    if depth == 0 || *budget == 0 {
        return None;
    }
    let children = ax_attr(el, "AXChildren")?;
    let arr = children.downcast::<CFArray>()?;
    for item in arr.iter() {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let child = *item as AXUIElementRef;
        let role =
            ax_attr(child, "AXRole").and_then(|r| r.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default();
        let has_range = ax_attr(child, "AXSelectedTextRange").is_some();
        if EDITABLE_ROLES.contains(&role.as_str()) || has_range {
            return target_from(child);
        }
        if let Some(found) = editable_descendant(child, depth - 1, budget) {
            return Some(found);
        }
    }
    None
}

/// Is this CF value the boolean true? (AX attributes like `AXFocused` come back as CFBoolean.)
fn is_true(value: Option<CFType>) -> bool {
    value.is_some_and(|v| v.downcast::<CFBoolean>() == Some(CFBoolean::true_value()))
}

fn read_cursor() -> Option<usize> {
    let focused = focused_element()?;
    ax_attr(focused.as_CFTypeRef(), "AXSelectedTextRange").as_ref().and_then(|r| range_location(r.as_CFTypeRef()))
}

/// Chromium/Electron apps (VS Code forks and friends) keep their accessibility tree unbuilt
/// until a client asks: with it off, the system-wide focused element reads as nil and delivery
/// would fall back to the clipboard. Flipping the app's `AXManualAccessibility` builds the
/// tree — the same thing macOS does when VoiceOver connects — after which the real focused
/// text area resolves as a normal editable element. No-op for apps without the attribute.
fn enable_chromium_accessibility() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    let app_info = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app_info.processIdentifier();
    if pid == std::process::id() as i32 {
        return None;
    }
    // SAFETY: returns a +1 retained application element, released below.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    let enabled = (|| {
        if is_true(ax_attr(app, "AXManualAccessibility")) {
            return true;
        }
        let mut names: CFArrayRef = std::ptr::null_mut();
        // SAFETY: `app` is live, `names` a valid out-pointer; +1 retained on success.
        if unsafe { AXUIElementCopyAttributeNames(app, &mut names) } != AX_SUCCESS || names.is_null() {
            return false;
        }
        // SAFETY: adopting the +1 retained CFArray.
        let names = unsafe { CFArray::<CFString>::wrap_under_create_rule(names) };
        if !names.iter().any(|n| n.to_string() == "AXManualAccessibility") {
            return false;
        }
        let attr = CFString::new("AXManualAccessibility");
        // SAFETY: live element, valid attribute name and CFBoolean value.
        let set = unsafe {
            AXUIElementSetAttributeValue(app, attr.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef())
        };
        set == AX_SUCCESS
    })();
    // SAFETY: `app` was created above and is not used afterwards.
    unsafe { CFRelease(app) };
    enabled.then_some(pid)
}

/// Kick the target app's accessibility machinery the moment recording starts, so a lazily
/// built tree (Electron/Chromium: GitHub Desktop, Feishu, …) is ready by delivery time. The
/// user speaks for seconds; the first attempt at delivery would otherwise pay the tree-build
/// latency and often lose the race.
pub fn warm_focus() {
    std::thread::spawn(|| {
        if !is_trusted() || read_focus().is_some() {
            return;
        }
        if let Some(pid) = enable_chromium_accessibility() {
            tracing::info!(
                "insert: frontmost app (pid {pid}) keeps its accessibility tree off; \
                            warming it during recording"
            );
        }
    });
}

/// Wait until the focused element stops changing AND is a deliverable target: editable, or a
/// key window that takes HID keys. Focus settles asynchronously after HUD interactions, app
/// switches and field clicks, and Chromium-family editors (Typora's fork, Electron) report a
/// generic container first while the real editable node materializes a moment later — so a
/// stable-but-generic read is not final and polling continues. On timeout, deliver to whatever
/// is focused then.
fn wait_for_stable_focus() -> Option<FocusTarget> {
    if !is_trusted() {
        return None;
    }
    let started = Instant::now();
    let mut deadline = started + FOCUS_BUDGET;
    let mut prev = read_focus();
    if prev.is_none()
        && let Some(pid) = enable_chromium_accessibility()
    {
        tracing::info!(
            "insert: frontmost app (pid {pid}) keeps its accessibility tree off; \
                        enabled it and waiting for the tree"
        );
        // The tree is built asynchronously; give Chromium room before falling back.
        deadline = Instant::now() + AX_TREE_BUDGET;
        prev = read_focus();
    }
    // A generic container may still resolve into an editable node; give the lazy tree the same
    // room before classifying it as undeliverable.
    if prev.as_ref().is_some_and(|t| !t.editable && !t.foreign_key_window()) {
        deadline = Instant::now() + AX_TREE_BUDGET;
    }
    while Instant::now() < deadline {
        std::thread::sleep(FOCUS_POLL);
        let cur = read_focus();
        let stable = matches!((&prev, &cur), (Some(a), Some(b)) if a.same_identity(b));
        let deliverable = |t: &FocusTarget| t.editable || t.foreign_key_window();
        let ready = cur.as_ref().is_some_and(deliverable);
        prev = cur;
        if stable && ready {
            return prev;
        }
    }
    if prev.is_some() {
        tracing::warn!("insert: focus still moving after {:?}; delivering anyway", started.elapsed());
    }
    prev
}

fn post_key(keycode: u16, down: bool, flags: Option<CGEventFlags>, text: Option<&[u16]>) -> bool {
    // A private source keeps posted events isolated from the physical keyboard state: a
    // modifier the user is still holding (right Command ending hold-mode, Shift, …) must not
    // bleed into synthesized keystrokes and turn them into shortcuts.
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::Private) else { return false };
    let Ok(ev) = CGEvent::new_keyboard_event(src, keycode, down) else { return false };
    if let Some(f) = flags {
        ev.set_flags(f);
    }
    if let Some(t) = text {
        ev.set_string_from_utf16_unchecked(t);
    }
    ev.post(CGEventTapLocation::HID);
    true
}

/// Post `units` as unicode key events (no clipboard involved). Returns false only when events
/// could not be posted at all — whether they land is verified separately.
fn post_type(units: &[u16]) -> bool {
    let mut ok = true;
    for chunk in Utf16Chunks::new(units, TYPE_CHUNK_UNITS) {
        ok &= post_key(0, true, None, Some(chunk));
        ok &= post_key(0, false, None, Some(chunk));
        std::thread::sleep(Duration::from_millis(TYPE_CHUNK_GAP_MS));
    }
    ok
}

/// Poll the target's selection until it moved by `expected` UTF-16 units, or verification runs
/// out. Returns the verdict and the last selection location seen (the next baseline).
///
/// `text` is exactly what this attempt inserts. Editors rewrite text as it arrives (smart
/// quotes, `...` → `…`, markdown spacing), so the cursor can move by fewer units than posted
/// even though everything landed. When the cursor verdict is a refute, the field's content gets
/// a second look before declaring failure.
fn verify(expected: usize, before: Option<usize>, text: &str) -> (Outcome, Option<usize>) {
    // Without a baseline the delta can never be observed, so the verdict is Unknown whatever
    // happens (key-window targets, secure fields) — the poll loop would only add latency.
    let Some(base) = before else { return (Outcome::Unknown, None) };
    let mut last = None;
    for _ in 0..VERIFY_TRIES {
        std::thread::sleep(VERIFY_POLL);
        let cur = read_cursor();
        if cur.is_some() {
            last = cur;
        }
        if let Some(a) = cur
            && a >= base
            && a - base >= expected
        {
            return (Outcome::Confirmed, cur);
        }
    }
    let verdict = outcome(expected, Some(base), last);
    if matches!(verdict, Outcome::Refuted { .. }) && landed_by_content(read_field_value().as_deref(), text) {
        tracing::info!("insert: cursor disagrees but the text is in the field; counting it as confirmed");
        return (Outcome::Confirmed, last);
    }
    (verdict, last)
}

/// Comparison fold for content verification: editors rewrite punctuation and whitespace as text
/// arrives, so only the word characters (letters, digits, CJK) carry reliable evidence.
fn fold_content(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The field's current value ends with (a rewritten form of) `text` — proof the insertion
/// landed even when the cursor moved less than expected. Short texts carry too little evidence
/// and stay with the cursor verdict.
fn landed_by_content(value: Option<&str>, text: &str) -> bool {
    let Some(value) = value else { return false };
    let needle = fold_content(text);
    needle.chars().count() >= 4 && fold_content(value).ends_with(&needle)
}

fn read_field_value() -> Option<String> {
    let el = focused_element()?;
    let v = ax_attr(el.as_CFTypeRef(), "AXValue")?;
    v.downcast::<CFString>().map(|s| s.to_string())
}

const KEYCODE_V: u16 = 9;

/// The whole pasteboard at one moment: every item's every type as raw bytes. A plain text
/// snapshot would silently drop everything else the user had copied (images, file references,
/// rich text).
type PasteboardItems = Vec<Vec<(String, Vec<u8>)>>;

/// Read every pasteboard item's every type. `None` when the pasteboard is unreadable, in which
/// case no restore happens and the dictated text simply stays on the clipboard.
fn snapshot_pasteboard() -> Option<PasteboardItems> {
    use objc2_app_kit::NSPasteboard;
    let pb = NSPasteboard::generalPasteboard();
    let items = pb.pasteboardItems()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        let mut types = Vec::new();
        for ty in item.types().iter() {
            let bytes = item.dataForType(&ty).map(|d| d.to_vec()).unwrap_or_default();
            types.push((ty.to_string(), bytes));
        }
        out.push(types);
    }
    Some(out)
}

/// Write `items` back to the pasteboard, replacing whatever is there now.
fn restore_pasteboard(items: &PasteboardItems) -> bool {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    let new_items: Vec<Retained<NSPasteboardItem>> = items
        .iter()
        .map(|types| {
            let item = NSPasteboardItem::new();
            for (ty, bytes) in types {
                let data = NSData::with_bytes(bytes);
                let ty = NSString::from_str(ty);
                item.setData_forType(&data, &ty);
            }
            item
        })
        .collect();
    let writing: Vec<&objc2::runtime::ProtocolObject<dyn NSPasteboardWriting>> =
        new_items.iter().map(|item| objc2::runtime::ProtocolObject::from_ref(&**item)).collect();
    // NSPasteboardItem implements NSPasteboardWriting.
    pb.writeObjects(&NSArray::from_slice(&writing))
}

/// Restores the user's clipboard after a paste attempt. `pasted_at` plus the floor keeps the
/// restore from racing the target app's asynchronous pasteboard read; the change-count guard
/// keeps it from clobbering a clipboard the user (or a clipboard manager) has rewritten since —
/// in that case their content wins and the dictated text stays where it is.
#[derive(Debug)]
struct PasteGuard {
    pasted_at: Instant,
    prev: Option<PasteboardItems>,
    /// The pasteboard's change count right after our own write.
    count_after_write: isize,
}

impl PasteGuard {
    fn remaining(&self, floor: Duration) -> Duration {
        floor.saturating_sub(self.pasted_at.elapsed())
    }

    fn restore(&self) {
        let Some(prev) = &self.prev else { return };
        use objc2_app_kit::NSPasteboard;
        let pb = NSPasteboard::generalPasteboard();
        if pb.changeCount() != self.count_after_write {
            tracing::info!("insert: clipboard changed since the paste; keeping the newer content");
            return;
        }
        restore_pasteboard(prev);
    }

    /// Restore in the background; delivery has been accepted.
    fn restore_after(self, floor: Duration) {
        let wait = self.remaining(floor);
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            self.restore();
        });
    }

    /// Restore on the spot; used before overwriting the clipboard with the fallback copy, which a
    /// pending background restore would clobber right after.
    fn restore_blocking(self, floor: Duration) {
        let wait = self.remaining(floor);
        std::thread::sleep(wait);
        self.restore();
    }
}

/// Put `text` on the clipboard and send Cmd+V. On failure the previous clipboard is restored
/// before returning; on success that is [`PasteGuard::restore_*`]'s job.
fn begin_paste(text: &str) -> Result<PasteGuard, String> {
    let prev = snapshot_pasteboard();
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())?;
    let count_after_write = {
        use objc2_app_kit::NSPasteboard;
        NSPasteboard::generalPasteboard().changeCount()
    };
    let posted = post_key(KEYCODE_V, true, Some(CGEventFlags::CGEventFlagCommand), None)
        && post_key(KEYCODE_V, false, Some(CGEventFlags::CGEventFlagCommand), None);
    if !posted {
        tracing::warn!("insert: could not send Cmd+V; restoring the clipboard");
        if let Some(prev) = &prev {
            restore_pasteboard(prev);
        }
        return Err("could not send Cmd+V".into());
    }
    Ok(PasteGuard { pasted_at: Instant::now(), prev, count_after_write })
}

/// Put `text` on the clipboard (also used by the history UI for its copy action).
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).map_err(|e| e.to_string())
}

fn terminal_clipboard(text: &str) -> Delivery {
    match copy_to_clipboard(text) {
        Ok(()) => Delivery::Copied,
        Err(e) => {
            tracing::warn!("insert: clipboard copy failed: {e}");
            Delivery::Failed(e)
        }
    }
}

/// Does the focused UI element accept typed text? Either a readable text field, or a key
/// window of another app that publishes no text tree but receives HID keys. (Needs the
/// Accessibility permission.)
pub fn has_text_target() -> bool {
    read_focus().is_some_and(|t| t.editable || t.foreign_key_window())
}

/// The delivery spine: wait for stable focus, then type/paste with verification and the
/// [`dito_core::insert`] fallback flow, ending on the clipboard if nothing live worked.
fn deliver_text(text: &str, method: InsertMethod) -> Delivery {
    let Some(target) = wait_for_stable_focus() else {
        tracing::warn!("insert: no readable focused element (Accessibility permission?); text left on clipboard");
        return terminal_clipboard(text);
    };
    if target.pid == std::process::id() as i32 {
        tracing::warn!(
            "insert: keyboard focus is on dito itself (role {:?}) — a HUD/settings click moved it; \
             text left on the clipboard",
            target.role
        );
        return terminal_clipboard(text);
    }
    if !target.editable {
        if !target.foreign_key_window() {
            tracing::warn!(
                "insert: focused element (pid {}, role {:?}) takes no text; text left on clipboard",
                target.pid,
                target.role
            );
            return terminal_clipboard(text);
        }
        tracing::info!(
            "insert: focus is the app's key window (pid {}, {:?}) with no AX text tree; \
             inserting without verification",
            target.pid,
            target.title
        );
    }

    let units: Vec<u16> = text.encode_utf16().collect();
    let mut flow = Flow::new(method);
    let mut step = Flow::first(method);
    let mut baseline = target.cursor;
    let mut guard = None;
    loop {
        let (result, last) = match step {
            Step::Type { from } => {
                let from = from.min(units.len());
                let suffix = &units[from..];
                tracing::debug!("insert: typing {} units from offset {from}", suffix.len());
                if !post_type(suffix) {
                    tracing::warn!("insert: could not post key events; nothing typed");
                    (Outcome::Refuted { inserted: 0 }, baseline)
                } else {
                    let suffix_text = String::from_utf16_lossy(suffix);
                    verify(suffix.len(), baseline, &suffix_text)
                }
            }
            Step::Paste => match begin_paste(text) {
                Ok(g) => {
                    guard = Some(g);
                    verify(units.len(), baseline, text)
                }
                Err(e) => {
                    tracing::warn!("insert: paste setup failed: {e}");
                    (Outcome::Refuted { inserted: 0 }, baseline)
                }
            },
        };
        if let Outcome::Refuted { inserted } = result {
            let expected = match step {
                Step::Type { from } => units.len().saturating_sub(from.min(units.len())),
                Step::Paste => units.len(),
            };
            tracing::warn!("insert: {step:?} attempt refuted — {inserted}/{expected} units landed");
        }
        match flow.advance(step, result) {
            Next::Done => {
                if matches!(result, Outcome::Unknown) {
                    tracing::info!("insert: accepted without verification (selection unreadable)");
                }
                if let Some(g) = guard.take() {
                    let floor =
                        if matches!(result, Outcome::Confirmed) { RESTORE_FLOOR } else { RESTORE_FLOOR_UNVERIFIED };
                    g.restore_after(floor);
                }
                return Delivery::Inserted;
            }
            Next::Attempt(next) => {
                tracing::warn!("insert: retrying with {next:?}");
                if let Some(g) = guard.take() {
                    g.restore_after(RESTORE_FLOOR);
                }
                baseline = last.or(baseline);
                step = next;
            }
            Next::CopyToClipboard => {
                tracing::warn!("insert: live insertion failed after retries; text left on clipboard");
                if let Some(g) = guard.take() {
                    g.restore_blocking(RESTORE_FLOOR);
                }
                return terminal_clipboard(text);
            }
        }
    }
}

/// The real text sink: insert at the cursor when possible, otherwise leave the text on the clipboard.
pub struct MacSink;

impl Sink for MacSink {
    fn deliver(&self, text: &str, method: InsertMethod) -> Delivery {
        deliver_text(text, method)
    }
}

#[cfg(test)]
mod tests {
    use super::landed_by_content;

    #[test]
    fn content_check_accepts_rewritten_punctuation() {
        // Feishu-style rewrites: smart quotes, ellipsis collapse, spacing around CJK.
        let value = "他说“你好”…世界, 你好 世界abc";
        assert!(landed_by_content(Some(value), "他说\"你好\"...世界, 你好 世界abc"));
        assert!(landed_by_content(Some(value), "你好 世界abc"));
    }

    #[test]
    fn content_check_rejects_missing_tail() {
        let value = "他说你好世界";
        assert!(!landed_by_content(Some(value), "你好世界再见"));
    }

    #[test]
    fn content_check_rejects_absent_value_and_short_text() {
        assert!(!landed_by_content(None, "你好世界很长"));
        // Short needles prove nothing.
        assert!(!landed_by_content(Some("嗯abc"), "嗯?"));
        assert!(!landed_by_content(Some("abc"), "abc"));
    }
}

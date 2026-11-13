use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message, class, msg_send};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSModalPanelWindowLevel, NSPanel, NSRunningApplication, NSScreen,
    NSStatusWindowLevel, NSView, NSWindow, NSWindowLevel, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::accessibility::reduce_motion;
use super::hud_follow::{Area, FOOTPRINT_BOTTOM, FollowFrame, HudFollowState, PointerContact, pointer_target};

/// Moves smaller than this are not worth a window server round trip.
const MIN_MOVE: f64 = 0.001;

/// The pointer in the HUD view's coordinates (origin top left), even while the window ignores mouse events.
#[derive(Debug, Clone, Copy)]
pub struct HudPointer {
    pub x: f32,
    pub y: f32,
    pub button_down: bool,
}

/// AppKit can call GPUI's move handler synchronously. Apply this after releasing the app borrow.
pub struct HudWindowMove(Retained<NSWindow>, NSPoint);

impl HudWindowMove {
    pub fn apply(self) {
        self.0.setFrameOrigin(self.1);
    }
}

/// The process that was in front when a command window took the keyboard; 0 when dito itself was, or none is known.
static SOURCE_APP: AtomicI32 = AtomicI32::new(0);
/// The process the front was handed back to, until it is seen in front again; 0 when nothing is pending.
static HANDED_BACK_TO: AtomicI32 = AtomicI32::new(0);

/// Without `ActivateAllWindows`, only the key window comes forward. The cooperative form (no options) is refused for
/// an app that is not in front.
#[allow(deprecated)]
const FORCE_FRONT: NSApplicationActivationOptions = NSApplicationActivationOptions::ActivateIgnoringOtherApps;

/// The level while a text window is open. An input method's candidate and composition windows are drawn in the
/// client's process at the Dock level (20, measured with `CGWindowListCopyWindowInfo` while composing Japanese and
/// pinyin), so the status level (25) the HUD otherwise uses would cover them. The modal panel level stays above app
/// windows and their floating panels, below the candidates.
const TEXT_WINDOW_LEVEL: NSWindowLevel = NSModalPanelWindowLevel;

fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace().frontmostApplication().map(|app| app.processIdentifier())
}

/// Input methods only compose for the app in front: the panel receives the keys while another app is in front, but the
/// system then offers them to that app's text client, and a Japanese or Chinese source types plain Latin. So dito takes
/// the front while a text window is open, and remembers who had it. Only the key window comes forward, so a settings
/// window behind stays behind.
fn take_front() {
    let own = std::process::id() as i32;
    let front = frontmost_pid();
    SOURCE_APP.store(front.filter(|pid| *pid != own).unwrap_or(0), Ordering::SeqCst);
    HANDED_BACK_TO.store(0, Ordering::SeqCst);
    if front != Some(own) {
        NSRunningApplication::currentApplication().activateWithOptions(FORCE_FRONT);
    }
}

/// Gives the front back to the app that had it, unless the user has already moved to another one. A window that closes
/// while it holds the front calls this too, since no key change reaches it then.
pub fn give_front_back() {
    let source = SOURCE_APP.swap(0, Ordering::SeqCst);
    if source == 0 || frontmost_pid() != Some(std::process::id() as i32) {
        return;
    }
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(source)
        && app.activateWithOptions(FORCE_FRONT)
    {
        HANDED_BACK_TO.store(source, Ordering::SeqCst);
    }
}

/// Whether the app that had the front before a text window took it has it again. `true` when nothing was handed
/// back. Activation takes a few frames; what a command writes goes to the app in front, so it waits for this.
pub fn source_app_in_front() -> bool {
    let pending = HANDED_BACK_TO.load(Ordering::SeqCst);
    let back = pending == 0 || frontmost_pid() == Some(pending);
    if back {
        HANDED_BACK_TO.store(0, Ordering::SeqCst);
    }
    back
}

/// Making a window key or resigning it fires gpui's key-status handler synchronously, and that handler needs the borrow
/// of the app that is still held inside a window update. Apply it after releasing that borrow, as for
/// [`HudWindowMove`]. Does nothing when the view had no window.
pub struct HudKeyChange(Option<Retained<NSWindow>>, bool);

impl HudKeyChange {
    pub fn apply(self) {
        let Self(Some(window), accepts) = self else { return };
        if let Ok(panel) = window.clone().downcast::<NSPanel>() {
            panel.setBecomesKeyOnlyIfNeeded(!accepts);
        }
        if accepts {
            window.setLevel(TEXT_WINDOW_LEVEL);
            if !window.isKeyWindow() {
                window.makeKeyWindow();
            }
            take_front();
        } else {
            window.setLevel(NSStatusWindowLevel);
            if window.isKeyWindow() {
                window.resignKeyWindow();
                if window.isKeyWindow() {
                    // Leaving the screen and coming back also hands key status to the app in front.
                    window.orderOut(None);
                    window.orderFrontRegardless();
                }
            }
            give_front_back();
        }
    }
}

/// AppKit calls gpui's resize and move handlers synchronously while the frame changes, and they need the borrow of
/// the app that is still held when the change is prepared. Apply it after releasing that borrow, as for
/// [`HudWindowMove`].
pub struct FramelessWindow {
    window: Retained<NSWindow>,
    view: Retained<NSView>,
    size: NSSize,
}

impl FramelessWindow {
    /// gpui opens every window titled, even without a title bar. Changing the style mask makes AppKit resign the
    /// window's key status and first responder; the first responder goes back to gpui's view here, as gpui does when
    /// it toggles its own borderless fullscreen, and the caller makes the window key again. gpui's window classes can
    /// become key whatever their style, so the window stays usable for keyboard input and text selection.
    pub fn apply(self) {
        let Self { window, view, size } = self;
        window.setStyleMask(NSWindowStyleMask::Borderless);
        window.setHasShadow(false);
        window.setFrame_display(NSRect::new(window.frame().origin, size), false);
        window.center();
        window.invalidateShadow();
        window.makeFirstResponder(Some(&view));
    }
}

/// Every method works on the view's current window and does nothing once the view has none.
pub struct HudNativeWindow<'a> {
    view: &'a NSView,
    mtm: MainThreadMarker,
}

impl<'a> HudNativeWindow<'a> {
    /// `None` off the main thread.
    ///
    /// # Safety
    /// `ns_view` must point to a live `NSView` (from a raw window handle) that stays alive for `'a`.
    pub unsafe fn from_ns_view(ns_view: NonNull<c_void>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        // SAFETY: the caller guarantees the pointer is a live NSView for `'a`.
        let view = unsafe { ns_view.cast::<NSView>().as_ref() };
        Some(Self { view, mtm })
    }

    fn window(&self) -> Option<Retained<NSWindow>> {
        self.view.window()
    }

    /// So a transparent window shows only what is drawn in it.
    pub fn remove_shadow(&self) {
        if let Some(window) = self.window() {
            window.setHasShadow(false);
            window.invalidateShadow();
        }
    }

    /// What is left after removing everything macOS draws is exactly what gpui paints, so a transparent window can be
    /// one card whose own corner is the only one. Applying the change makes the window `size` points large (width,
    /// height) and centers it; it does not show, order or activate it.
    pub fn frameless(&self, size: (f64, f64)) -> Option<FramelessWindow> {
        Some(FramelessWindow { window: self.window()?, view: self.view.retain(), size: NSSize::new(size.0, size.1) })
    }

    /// Returns the level AppKit reports afterwards, or `None` if the view has no window.
    ///
    /// gpui has no `WindowOptions` field for the level; its `WindowKind::Floating` also drops the mouse tracking area
    /// it installs for normal windows, which an unfocused window needs for hover.
    pub fn raise_to_status_level(&self) -> Option<NSWindowLevel> {
        self.window().map(|window| {
            window.setLevel(NSStatusWindowLevel);
            window.level()
        })
    }

    pub fn keep_panel_inactive(&self) {
        if let Some(panel) = self.window().and_then(|window| window.downcast::<NSPanel>().ok()) {
            panel.setBecomesKeyOnlyIfNeeded(true);
            panel.setHidesOnDeactivate(false);
        }
    }

    /// `true`: the panel can become key and is made key by [`HudKeyChange::apply`], and dito takes the front (the app
    /// that had it is remembered), so a text field in it receives typing and input methods compose. `false`: back to
    /// the state [`Self::keep_panel_inactive`] sets; a panel that is key gives key status up and the app that had the
    /// front gets it back, so its key window is key again.
    pub fn set_accepts_key(&self, accepts: bool) -> HudKeyChange {
        HudKeyChange(self.window(), accepts)
    }

    pub fn pointer(&self) -> Option<HudPointer> {
        let window = self.window()?;
        let p = window.mouseLocationOutsideOfEventStream();
        let bounds = self.view.bounds();
        Some(HudPointer {
            x: p.x as f32,
            y: (bounds.size.height - p.y) as f32,
            button_down: pressed_mouse_buttons() != 0,
        })
    }

    pub fn set_click_through(&self, through: bool) {
        if let Some(window) = self.window()
            && window.ignoresMouseEvents() != through
        {
            window.setIgnoresMouseEvents(through);
        }
    }

    /// Centers on the screen under the pointer, so the HUD changes screens with it.
    pub fn center_above_dock(&self, clearance: i32) -> Option<HudWindowMove> {
        let window = self.window()?;
        let screen = screen_under(mouse_location(), self.mtm).or_else(|| window.screen())?;
        let screen_frame = screen.frame();
        let visible = screen.visibleFrame();
        let frame = window.frame();
        let x = screen_frame.origin.x + (screen_frame.size.width - frame.size.width) / 2.;
        let y = (visible.origin.y + f64::from(clearance) - FOOTPRINT_BOTTOM)
            .min(visible.origin.y + (visible.size.height - frame.size.height).max(0.));
        ((frame.origin.x - x).abs() > MIN_MOVE || (frame.origin.y - y).abs() > MIN_MOVE)
            .then(|| HudWindowMove(window, NSPoint::new(x, y)))
    }

    pub fn follow_pointer(&self, state: &mut HudFollowState, contact: PointerContact) -> Option<HudWindowMove> {
        let window = self.window()?;
        let cursor = mouse_location();
        let frame = window.frame();
        let screen = screen_under(cursor, self.mtm).or_else(|| window.screen())?;
        let area = area_of(screen.visibleFrame());
        let cursor = (cursor.x, cursor.y);
        let (target, bounds) = pointer_target(cursor, frame.size.width, area);
        let current = state.position.unwrap_or((frame.origin.x, frame.origin.y));
        let follow = FollowFrame {
            cursor,
            window_width: frame.size.width,
            target,
            bounds,
            contact,
            reduced_motion: reduce_motion(),
        };
        let next = state.step(current, &follow, Instant::now())?;
        ((frame.origin.x - next.0).abs() > MIN_MOVE || (frame.origin.y - next.1).abs() > MIN_MOVE)
            .then(|| HudWindowMove(window, NSPoint::new(next.0, next.1)))
    }
}

/// The pointer in global coordinates with the origin at the primary screen's top left, as gpui reports display
/// bounds. `None` off the main thread.
pub fn pointer_location() -> Option<(f32, f32)> {
    let mtm = MainThreadMarker::new()?;
    let primary = NSScreen::screens(mtm).into_iter().next()?.frame();
    let cursor = mouse_location();
    Some((cursor.x as f32, (primary.size.height - cursor.y) as f32))
}

fn area_of(rect: NSRect) -> Area {
    Area { x: rect.origin.x, y: rect.origin.y, width: rect.size.width, height: rect.size.height }
}

fn screen_under(cursor: NSPoint, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    NSScreen::screens(mtm).into_iter().find(|screen| {
        let r = screen.frame();
        cursor.x >= r.origin.x
            && cursor.x < r.origin.x + r.size.width
            && cursor.y >= r.origin.y
            && cursor.y < r.origin.y + r.size.height
    })
}

fn mouse_location() -> NSPoint {
    // SAFETY: `+[NSEvent mouseLocation]` takes no arguments and returns an NSPoint.
    unsafe { msg_send![class!(NSEvent), mouseLocation] }
}

fn pressed_mouse_buttons() -> usize {
    // SAFETY: `+[NSEvent pressedMouseButtons]` takes no arguments and returns an NSUInteger bit mask.
    unsafe { msg_send![class!(NSEvent), pressedMouseButtons] }
}

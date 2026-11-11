use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message, class, msg_send};
use objc2_app_kit::{NSPanel, NSScreen, NSStatusWindowLevel, NSView, NSWindow, NSWindowLevel, NSWindowStyleMask};
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

    pub fn center_above_dock(&self, clearance: i32) -> Option<HudWindowMove> {
        let window = self.window()?;
        let screen = window.screen()?;
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

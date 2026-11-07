use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, class, msg_send};
use objc2_app_kit::{NSPanel, NSScreen, NSStatusWindowLevel, NSView, NSWindow, NSWindowLevel};
use objc2_foundation::{NSPoint, NSRect};

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

/// The HUD's native window, reached through the view gpui renders into. Every method works on the
/// view's current window and does nothing once the view has none.
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

    /// Turn off the drop shadow macOS gives every window, so a transparent window shows only what is drawn in it.
    pub fn remove_shadow(&self) {
        if let Some(window) = self.window() {
            window.setHasShadow(false);
            window.invalidateShadow();
        }
    }

    /// Raise the window to the macOS status level, above every application's normal windows and
    /// floating panels but below menus and tooltips, so it stays visible while the user works in
    /// other apps. Returns the level AppKit reports afterwards, or `None` if the view has no window.
    ///
    /// gpui has no `WindowOptions` field for the level; its `WindowKind::Floating` also drops the
    /// mouse tracking area it installs for normal windows, which an unfocused window needs for hover.
    pub fn raise_to_status_level(&self) -> Option<NSWindowLevel> {
        self.window().map(|window| {
            window.setLevel(NSStatusWindowLevel);
            window.level()
        })
    }

    /// Keep a non-activating HUD panel from becoming the key window when its controls are clicked.
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

    /// Move the HUD with a continuous spring while allowing the pointer to approach and operate it.
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

//! Opening the wizard window.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dito_platform::system::HudNativeWindow;
use gpui_kit::prelude::*;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::state::AppState;
use crate::ui::Root;

use super::{Onboarding, Step};

pub struct WizardWindow(pub WindowHandle<Root>, pub WeakEntity<Onboarding>);
impl Global for WizardWindow {}

/// The editor owns its bounds; the HUD only borrows an anchor while this field is the active target.
pub(crate) fn try_anchor(cx: &mut App) -> Option<Point<Pixels>> {
    let (handle, view) = cx.try_global::<WizardWindow>().map(|w| (w.0, w.1.clone()))?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                if view.step != Step::Try
                    || !window.is_window_active()
                    || !view.try_input.read(cx).focus_handle(cx).contains_focused(window, cx)
                {
                    return None;
                }
                let rect = view.try_bounds.get()?;
                let handle = HasWindowHandle::window_handle(window).ok()?;
                let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
                // SAFETY: the window borrow keeps its NSView alive through the conversion.
                let native = unsafe { HudNativeWindow::from_ns_view(handle.ns_view) }?;
                let (x, y) = native.point_on_screen(f32::from(rect.center().x), f32::from(rect.bottom() - px(16.)))?;
                Some(point(px(x), px(y)))
            })
            .ok()
            .flatten()
        })
        .ok()
        .flatten()
}

pub(crate) fn insert_try(text: &str, claimed: &AtomicBool, cx: &mut App) -> bool {
    let Some((handle, view)) = cx.try_global::<WizardWindow>().map(|w| (w.0, w.1.clone())) else {
        return false;
    };
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                if view.step != Step::Try
                    || !window.is_window_active()
                    || !view.try_input.read(cx).focus_handle(cx).contains_focused(window, cx)
                    || claimed.swap(true, Ordering::SeqCst)
                {
                    return false;
                }
                view.try_input.update(cx, |field, cx| field.replace(text.to_owned(), window, cx));
                view.tried_ok = true;
                view.seen = [true; 3];
                cx.notify();
                true
            })
            .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(feature = "screenshots")]
pub fn open_with_view(st: Arc<AppState>, cx: &mut App) -> Option<(WindowHandle<Root>, Entity<Onboarding>)> {
    let bounds = Bounds::centered(None, size(px(1040.), px(720.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(20.), px(20.))),
        }),
        focus: false,
        show: false,
        ..Default::default()
    };
    let mut slot = None;
    let h = cx
        .open_window(opts, |window, cx| {
            let view = cx.new(|cx| Onboarding::new(st, window, cx));
            slot = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx))
        })
        .ok()?;
    slot.map(|v| (h, v))
}

/// The wizard keeps its global after the window closes, so ask the app whether the window is still there.
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<WizardWindow>().is_some_and(|h| cx.windows().iter().any(|w| w.window_id() == h.0.window_id()))
}

pub fn restart(st: Arc<AppState>, cx: &mut App) {
    if let Some(h) = cx.try_global::<WizardWindow>().map(|h| h.0)
        && h.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        cx.activate(true);
        return;
    }
    open(st, cx);
}

pub fn open(st: Arc<AppState>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1040.), px(720.)), cx);
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Dito".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(20.), px(20.))),
        }),
        is_resizable: false,
        focus: !st.opts.is_headless(),
        show: !st.opts.is_headless(),
        ..Default::default()
    };
    let foreground = !st.opts.is_headless();
    let mut view_slot = None;
    match cx.open_window(opts, |window, cx| {
        let view = cx.new(|cx| {
            let mut wizard = Onboarding::new(st, window, cx);
            wizard.begin_intro(cx);
            wizard
        });
        view_slot = Some(view.downgrade());
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        Ok(h) => cx.set_global(WizardWindow(h, view_slot.expect("the wizard view is built with its window"))),
        Err(e) => tracing::error!("could not open onboarding: {e}"),
    }
    if foreground {
        crate::dock::sync(cx);
        cx.activate(true);
    }
}

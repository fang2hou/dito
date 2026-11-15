use gpui_kit::*;

use crate::settings::{self, Settings};
use crate::tools::{self, Tools};

/// Run `f` against the open settings window and its view inside one UI update, so native resize and mouse events
/// cannot interleave with a simulated pointer sequence.
pub fn on_settings_window(cx: &mut AsyncApp, f: impl FnOnce(&mut Window, &mut App, &Entity<Settings>)) {
    cx.update(|cx| {
        let (window, view) = settings::handle(cx).expect("settings window");
        cx.update_window(window.into(), |_, w, cx| f(w, cx, &view)).expect("settings window update");
    });
}

/// Run `f` against the open tools window and its view inside one UI update, so native resize and mouse events
/// cannot interleave with a simulated pointer sequence.
pub fn on_tools_window(cx: &mut AsyncApp, f: impl FnOnce(&mut Window, &mut App, &Entity<Tools>)) {
    cx.update(|cx| {
        let (window, view) = tools::handle(cx).expect("tools window");
        cx.update_window(window.into(), |_, w, cx| f(w, cx, &view)).expect("tools window update");
    });
}

pub fn redraw(w: &mut Window, cx: &mut App) {
    w.refresh();
    w.draw(cx).clear(cx);
}

/// Draws after the move, so hover state is current before the next event.
pub fn hover(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), cx);
    redraw(w, cx);
}

pub fn drag_to(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    let event = MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() };
    w.dispatch_event(PlatformInput::MouseMove(event), cx);
    redraw(w, cx);
}

pub fn press(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    let event = MouseDownEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() };
    w.dispatch_event(PlatformInput::MouseDown(event), cx);
}

pub fn release(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    let event = MouseUpEvent { position, button: MouseButton::Left, click_count: 1, ..Default::default() };
    w.dispatch_event(PlatformInput::MouseUp(event), cx);
}

pub fn click(w: &mut Window, cx: &mut App, position: Point<Pixels>) {
    hover(w, cx, position);
    press(w, cx, position);
    release(w, cx, position);
}

pub fn press_key(w: &mut Window, cx: &mut App, keystroke: &'static str) {
    let keystroke = Keystroke::parse(keystroke).expect("keystroke");
    let event = KeyDownEvent { keystroke, is_held: false, prefer_character_input: false };
    w.dispatch_event(PlatformInput::KeyDown(event), cx);
}

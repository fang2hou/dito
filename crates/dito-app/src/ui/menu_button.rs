use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::callbacks::Cb;
use super::hover::{hover_fill, hover_tracker};
use super::primitives::icon;
use super::scope::{ScopeHandle, motion_scope};
use crate::assets::Ic;
use crate::theme::Palette;

/// A press on a button whose menu is open already closes that menu (it closes on any press outside itself,
/// including this button's). Remember it under the scope the button was built in, so the click that follows
/// does not toggle the menu back open.
pub(super) fn press_mark(
    scope: ScopeHandle,
    key: ElementId,
    open: bool,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    move |_, _, _| {
        if let Some(reg) = scope.registry() {
            let mut reg = reg.borrow_mut();
            if open {
                reg.pressed_open.insert(key.clone());
            } else {
                reg.pressed_open.remove(&key);
            }
        }
    }
}

/// Whether the click should toggle: the scope it was built under must still be live (a late click from a
/// retired page cannot open its menu), and the button must not have been pressed while its menu was open.
fn should_toggle(scope: &ScopeHandle, key: &ElementId) -> bool {
    scope.registry().is_some_and(|reg| !reg.borrow_mut().pressed_open.remove(key))
}

pub(super) fn press_toggle(
    scope: ScopeHandle,
    key: ElementId,
    toggle: Cb,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, w, cx| {
        if should_toggle(&scope, &key) {
            toggle(w, cx)
        }
    }
}

/// Circular ⋮-style button that opens a menu: like [`icon_button`], but a press while the menu is open closes it.
pub fn menu_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, open: bool, toggle: Cb) -> Stateful<Div> {
    menu_button_sized(id, p, ic, 16., open, toggle)
}

/// Circular menu control with a context-specific optical icon size.
pub fn menu_button_sized(
    id: impl Into<ElementId>,
    p: &Palette,
    ic: Ic,
    icon_size: f32,
    open: bool,
    toggle: Cb,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let scope = motion_scope();
    div()
        .id(id.clone())
        .size(px(CONTROL_H))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(p.border_strong)
        .bg(if open { p.seg } else { p.card })
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, press_mark(scope.clone(), id.clone(), open))
        .on_click(press_toggle(scope, id.clone(), toggle))
        .on_hover(hover_tracker(id.clone()))
        .relative()
        .child(hover_fill(&id, None, p.seg))
        .child(icon(ic, icon_size, p.text))
}

#[cfg(test)]
mod tests {
    use super::{ElementId, should_toggle};
    #[test]
    fn a_click_from_an_expired_scope_never_toggles_the_menu() {
        let mut scope = crate::ui::MotionScope::new();
        let key = ElementId::from("menu");
        // Pressed while the menu was open: the following click must not re-open it.
        scope.handle().registry().unwrap().borrow_mut().pressed_open.insert(key.clone());
        assert!(!should_toggle(&scope.handle(), &key), "the press-while-open mark suppresses the toggle");
        assert!(!scope.handle().registry().unwrap().borrow().pressed_open.contains(&key), "the mark is consumed");
        assert!(should_toggle(&scope.handle(), &key), "a clean live scope toggles");
        // The page the button was built in has been reset (navigation, window close): its late
        // click can neither read the mark nor open the menu again.
        let stale = scope.handle();
        scope.reset();
        assert!(stale.expired(), "the reset retired the captured handle");
        assert!(!should_toggle(&stale, &key), "an expired scope never toggles");
    }
}

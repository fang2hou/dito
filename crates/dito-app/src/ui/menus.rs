use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::callbacks::{Cb, IndexCb};
use super::controls::{CONTROL_H, hover_fill};
use super::inputs::field_chrome;
use super::motion::{HOVER_MS, RING_MS, ScopeHandle, count_flips, hover_tracker, is_hovered, motion_scope, tweened};
use super::primitives::{hex_alpha, icon};
use super::tooltip::{tip_above, tip_label};
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

/// A press on a button whose menu is open already closes that menu (it closes on any press outside itself,
/// including this button's). Remember it under the scope the button was built in, so the click that follows
/// does not toggle the menu back open.
fn press_mark(
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

fn press_toggle(
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

/// 36px circular ⋮-style button that opens a menu: like [`icon_button`], but a press while the menu is open closes it.
pub fn menu_button(id: impl Into<ElementId>, p: &Palette, ic: Ic, open: bool, toggle: Cb) -> Stateful<Div> {
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
        .child(icon(ic, 16., p.text))
}

/// The select's arrow: points down when closed and turns to point up while the menu is open, on the same
/// clock as the menu's appearance.
fn chevron(id: ElementId, color: Hsla, open: bool) -> AnyElement {
    let flips = count_flips(&id, open, Duration::from_millis(RING_MS));
    let at = move |f: f32| {
        icon(Ic::ChevronDown, 14., color).with_transformation(Transformation::rotate(radians(std::f32::consts::PI * f)))
    };
    let settled = if open { 1. } else { 0. };
    if flips == 0 {
        return at(settled).into_any_element();
    }
    at(settled)
        .with_animation(
            (id, SharedString::from(format!("turn-{flips}"))),
            Animation::new(Duration::from_millis(RING_MS)),
            move |_, t| {
                let q = t * t * (3. - 2. * t);
                at(if open { q } else { 1. - q })
            },
        )
        .into_any_element()
}

/// An inert select: same chrome, muted content, no menu. For the "nothing to pick" case (e.g. no
/// AWS profiles configured) — the control must not look operable.
pub fn select_disabled(p: &Palette, label: SharedString) -> Div {
    field_chrome((ElementId::from("select-disabled"), "ring"), p, false)
        .w_full()
        .px(px(12.))
        .justify_between()
        .opacity(0.6)
        .child(text(label, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)))
        .child(icon(Ic::ChevronDown, 14., p.muted))
}

/// Drop-down: a bordered button that opens a list below it. The menu is never narrower than the
/// button; longer options (device names) may widen it beyond.
#[allow(clippy::too_many_arguments)]
pub fn select(
    id: &'static str,
    p: &Palette,
    options: &[SharedString],
    selected: usize,
    open: bool,
    width: Option<f32>,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let items: Vec<SelectItem> = options.iter().map(|o| SelectItem { label: o.clone(), badge: None }).collect();
    select_items(id, p, &items, selected, open, width, on_toggle, on_pick)
}

/// A small block toggle at the right of a menu row: a ban icon, red while `active`, gray otherwise.
/// Clicking it flips its own state and leaves the menu open; it never picks the row.
pub struct SelectBadge {
    pub active: bool,
    pub tip: SharedString,
    pub on_click: Cb,
}

pub struct SelectItem {
    pub label: SharedString,
    pub badge: Option<SelectBadge>,
}

fn select_badge(id: (&'static str, usize), p: &Palette, b: &SelectBadge) -> Stateful<Div> {
    let fg = match (b.active, p.dark) {
        (false, _) => hex_alpha(p.muted, 0.4),
        (true, true) => p.danger,
        (true, false) => p.danger_text,
    };
    let cb = b.on_click.clone();
    let tip_id = ElementId::from((ElementId::from(id), "badge"));
    tip_label(tip_id.clone(), b.tip.clone());
    let hover = hex_alpha(p.text, 0.08);
    // Keep the fill and glyph in one normal-flow box. Both sizes are even so centering
    // does not introduce half-pixel offsets that SVG pixel snapping rounds differently.
    div()
        .id(tip_id.clone())
        .relative()
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .cursor_pointer()
        .on_hover(hover_tracker(tip_id.clone()))
        .child(tweened(
            ElementId::from((tip_id.clone(), "hover-fill")),
            if is_hovered(&tip_id) { 1. } else { 0. },
            HOVER_MS,
            move |v| {
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(Hsla { a: hover.a * v, ..hover })
                    .child(icon(Ic::Ban, 16., fg))
            },
        ))
        .child(tip_above(p, &tip_id))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, w, cx| {
            cx.stop_propagation();
            cb(w, cx);
        })
}

/// [`select`] with per-row badges.
#[allow(clippy::too_many_arguments)]
pub fn select_items(
    id: &'static str,
    p: &Palette,
    options: &[SelectItem],
    selected: usize,
    open: bool,
    width: Option<f32>,
    on_toggle: Cb,
    on_pick: IndexCb,
) -> Div {
    let label = options.get(selected).map(|o| o.label.clone()).unwrap_or_default();
    let toggle = on_toggle.clone();
    let scope = motion_scope();
    let mut button = field_chrome((ElementId::from(id), "ring"), p, open)
        .id(id)
        .px(px(12.))
        .cursor_pointer()
        .justify_between()
        // The open menu closes itself on any press outside it, including this button's; so a click that began
        // while the menu was open must not toggle it back open.
        .on_hover(hover_tracker(ElementId::from(id)))
        .child(hover_fill(&ElementId::from(id), Some(12.), hex_alpha(p.seg, 0.55)))
        .on_mouse_down(MouseButton::Left, press_mark(scope.clone(), ElementId::from(id), open))
        .on_click(press_toggle(scope.clone(), ElementId::from(id), toggle))
        .gap(px(8.))
        // `truncate` clips both axes. A font-size-high line box cuts off Outfit descenders
        // (notably the g in English); reserve a full 20 pt line and center it in the control.
        .child(text(label, 13., FontWeight::NORMAL, p.text).line_height(px(20.)).min_w(px(0.)).truncate())
        .child(chevron(ElementId::from((ElementId::from(id), "chevron")), p.text, open));
    button = match width {
        Some(w) => button.w(px(w)),
        None => button.w_full(),
    };
    let mut root = div().relative().when(width.is_none(), |d| d.w_full()).child(button);
    if width.is_none() {
        // Fill-width buttons resolve their width at layout time; record it every paint so the menu
        // panel (built one frame ahead) can match it. The paint closure captures the build scope's
        // handle: the ambient scope no longer applies while painting.
        let paint_scope = scope.clone();
        root = root.child(
            canvas(
                move |bounds, _, _| {
                    if let Some(reg) = paint_scope.registry() {
                        reg.borrow_mut().select_w.insert(id, f32::from(bounds.size.width));
                    }
                },
                |_, _, _, _| (),
            )
            .absolute()
            .size_full(),
        );
    }
    if open {
        let close = on_toggle.clone();
        let floor = width
            .unwrap_or_else(|| scope.registry().and_then(|reg| reg.borrow().select_w.get(id).copied()).unwrap_or(200.));
        let mut panel = div()
            .id((ElementId::from(id), "menu"))
            .min_w(px(floor.max(180.)))
            .max_w(px(400.))
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .rounded(px(12.))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .shadow(vec![BoxShadow {
                color: p.shadow,
                offset: point(px(0.), px(10.)),
                blur_radius: px(28.),
                spread_radius: px(0.),
                inset: false,
            }])
            .occlude()
            .on_mouse_down_out(move |_, w, cx| close(w, cx));
        // The panel only exists while open, so this runs each time it appears.
        for (i, o) in options.iter().enumerate() {
            let pick = on_pick.clone();
            let toggle = on_toggle.clone();
            let on = i == selected;
            panel = panel.child(
                div()
                    .id((id, i))
                    .h(px(34.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    // Concentric with the panel (12 − 1 border − 6 padding), so the first/last rows
                    // hug the menu's own corners instead of showing a step.
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(p.seg))
                    .on_click(move |_, w, cx| {
                        pick(i, w, cx);
                        toggle(w, cx);
                    })
                    // The label takes the leftover width and truncates; badge and check keep their own
                    // fixed slots, so a long name can never run under them.
                    .child(
                        text(o.label.clone(), 13., FontWeight::NORMAL, p.text)
                            .line_height(px(20.))
                            .flex_1()
                            .min_w(px(0.))
                            .truncate(),
                    )
                    .children(o.badge.as_ref().map(|b| select_badge((id, i), p, b)))
                    .child(div().size(px(14.)).flex_none().when(on, |d| d.child(icon(Ic::Check, 14., p.accent_text)))),
            );
        }
        root = root.child(float(ElementId::from((ElementId::from(id), "menu")), CONTROL_H + 6., panel));
    }
    root
}

/// Places `panel` `top` px under its relative parent with the right edges aligned. It is drawn in the
/// deferred layer: above siblings and not clipped by scroll or overflow containers; it only snaps to the
/// window edges.
fn float<E: IntoElement + Styled + 'static>(id: ElementId, top: f32, panel: E) -> Div {
    // Fades in and settles 6px upwards on the same clock and easing as the focus ring.
    let appear = panel.with_animation((id, "appear"), Animation::new(Duration::from_millis(RING_MS)), |p, t| {
        let q = t * t * (3. - 2. * t);
        p.opacity(q).relative().top(px((1. - q) * -6.))
    });
    div().absolute().top(px(top)).right(px(0.)).size(px(0.)).child(
        deferred(anchored().anchor(Anchor::TopRight).snap_to_window_with_margin(px(8.)).child(appear)).with_priority(1),
    )
}

/// Accent for a menu entry: neutral by default, colored like the danger entries when the
/// action carries meaning worth seeing at a glance (start = green, restart = amber).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuTone {
    #[default]
    Default,
    Danger,
    Success,
    // Reserved by the unwired HistoryKind::Cancelled menu design.
    #[allow(dead_code)]
    Warning,
}

pub struct MenuItem {
    pub icon: Option<Ic>,
    pub label: SharedString,
    pub tone: MenuTone,
    pub on_click: Cb,
}

/// Floating menu under its (relative) parent, right-aligned, drawn above everything else.
pub fn popup_menu(id: impl Into<ElementId>, p: &Palette, items: Vec<MenuItem>, on_close: Cb) -> Div {
    let id: ElementId = id.into();
    let mut m = div()
        .id(id.clone())
        .w(px(200.))
        .p(px(6.))
        .flex()
        .flex_col()
        .gap(px(1.))
        .rounded(px(12.))
        .bg(p.menu_bg)
        .border_1()
        .border_color(p.border)
        .shadow(vec![BoxShadow {
            color: p.shadow,
            offset: point(px(0.), px(10.)),
            blur_radius: px(28.),
            spread_radius: px(0.),
            inset: false,
        }])
        .occlude()
        .on_mouse_down_out({
            let c = on_close.clone();
            move |_, w, cx| c(w, cx)
        });
    for (i, it) in items.into_iter().enumerate() {
        let fg = match it.tone {
            MenuTone::Danger => {
                if p.dark {
                    p.danger
                } else {
                    p.danger_text
                }
            }
            MenuTone::Success => p.success,
            MenuTone::Warning => p.warning_text,
            MenuTone::Default => p.text,
        };
        let (cb, close) = (it.on_click, on_close.clone());
        m = m.child(
            div()
                .id((id.clone(), SharedString::from(format!("m{i}"))))
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                // Concentric with the panel (12 − 1 border − 6 padding); see `select`.
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(p.seg))
                .on_click(move |_, w, cx| {
                    close(w, cx);
                    cb(w, cx);
                })
                .children(
                    it.icon.map(|ic| icon(ic, 16., if matches!(it.tone, MenuTone::Default) { p.muted } else { fg })),
                )
                .child(text(it.label, 13., FontWeight::NORMAL, fg).line_height(relative(1.))),
        );
    }
    float(ElementId::from((id.clone(), "popup")), CONTROL_H + 8., m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn a_click_from_an_expired_scope_never_toggles_the_menu() {
        let mut scope = crate::ui::motion::MotionScope::new();
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

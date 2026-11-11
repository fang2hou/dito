use gpui_kit::*;

use crate::theme::Palette;
use crate::ui;

/// Like [`page`] but without the page-level scroll: for bodies that manage their own scrolling
/// (virtualized lists) and need the remaining height.
pub fn page_fill(
    p: Palette,
    id: &'static str,
    title: SharedString,
    actions: Option<AnyElement>,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .size_full()
        .px(px(56.))
        .pt(px(44.))
        .pb(px(36.))
        .flex()
        .flex_col()
        .gap(px(28.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.))
                .child(ui::page_title(&p, title))
                .children(actions),
        )
        .child(body)
}

pub fn page(
    p: Palette,
    id: &'static str,
    title: SharedString,
    actions: Option<AnyElement>,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .size_full()
        .overflow_y_scroll()
        .px(px(56.))
        .pt(px(44.))
        .pb(px(36.))
        .flex()
        .flex_col()
        .gap(px(28.))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.))
                .child(ui::page_title(&p, title))
                .children(actions),
        )
        .child(body)
}

/// Width of the page content: the window minus the sidebar (232) and the page padding (2 x 56).
pub fn content_width(window: &Window) -> f32 {
    f32::from(window.viewport_size().width) - 232. - 112.
}

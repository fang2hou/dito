//! The frame both windows draw: the sidebar, the page column with its wordmark and drag gutters, and
//! the layers that float above the page.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::health;
use super::nav::NavHost;
use super::sidebar::{rail_hover, sidebar};
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui;

/// The window's root element, built after [`super::Chrome::begin_frame`]. A window with page overlays
/// appends them to the result, so they paint above everything here.
///
/// `title` is the page's English name, drawn as the wordmark. `None` is a page that lays out its own
/// edges: its column carries no backdrop, wordmark or drag gutters.
pub(crate) fn frame<H: NavHost>(
    host: &H,
    c: &Ctx,
    title: Option<&'static str>,
    page: impl IntoElement,
    window: &Window,
    cx: &mut Context<H>,
) -> Div {
    let p = c.p;
    let rail = host.chrome().rail();
    div()
        .size_full()
        .flex()
        .bg(p.bg)
        .text_color(p.text)
        .font_family(ui::cjk_family())
        // Any click first drops keyboard focus; an input under the pointer then takes it back.
        .capture_any_mouse_down(|_, window, cx| window.blur(cx))
        .child(sidebar(host, c, cx, rail))
        .child(page_column(title, page, p))
        // Last child: the rail's floating reveal paints above the page content.
        .when(rail, |el| el.child(rail_hover(host, c, cx)))
        // Painted last so the health card floats above the page content, not under it.
        .child(health::overlay(host.chrome(), c, window))
}

fn page_column(title: Option<&'static str>, page: impl IntoElement, p: Palette) -> Div {
    let column = div().flex_1().h_full().relative().overflow_hidden();
    let Some(title) = title else { return column.child(page) };
    column
        .child(ui::prepare_card_backdrop(title, p.ui))
        .child(ui::page_wordmark(title, p.ui))
        .child(page)
        // Scroll still passes through, and content never enters these zones, so nothing is blocked.
        .child(ui::drag_area("gutter-left").absolute().top_0().left_0().h_full().w(px(56.)).block_mouse_except_scroll())
        .child(
            ui::drag_area("gutter-right").absolute().top_0().right_0().h_full().w(px(56.)).block_mouse_except_scroll(),
        )
}

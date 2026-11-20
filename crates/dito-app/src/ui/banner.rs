use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::{BtnKind, button};
use super::callbacks::Cb;
use super::primitives::{hex_alpha, icon};
use super::typography::text;
use crate::ui::Ic;
use crate::ui::Palette;

pub struct BannerAction {
    pub id: ElementId,
    pub label: SharedString,
    pub on_click: Cb,
}

/// Numbers from `design:settings/wordbook/failure-banner`.
pub fn banner(p: &Palette, message: impl Into<SharedString>, action: Option<BannerAction>) -> Div {
    let (bg, border, icon_color, fg) = if p.dark {
        (hex_alpha(p.danger, 0.12), hex_alpha(p.danger, 0.5), p.danger, p.text)
    } else {
        (p.danger_soft, p.danger, p.danger_text, p.danger_text)
    };
    let mut strip = div()
        .w_full()
        .pl(px(20.))
        .pr(px(12.))
        .py(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .rounded(px(16.))
        .bg(bg)
        .border_1()
        .border_color(border)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(12.))
                .child(icon(Ic::Alert, 18., icon_color))
                .child(text(message, 13., FontWeight::MEDIUM, fg).flex_1().min_w(px(0.))),
        );
    if let Some(BannerAction { id, label, on_click }) = action {
        strip = strip.child(button(id, p, &label, BtnKind::Secondary, None, on_click));
    }
    strip
}

pub fn dismissible_banner(
    p: &Palette,
    id: impl Into<ElementId>,
    message: impl Into<SharedString>,
    close_label: impl Into<SharedString>,
    on_close: Cb,
) -> Div {
    banner(p, message, Some(BannerAction { id: id.into(), label: close_label.into(), on_click: on_close }))
}

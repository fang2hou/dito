use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::callbacks::Cb;
use super::menu_panel::{float, menu_panel, menu_row};
use super::primitives::icon;
use super::typography::text;
use crate::assets::Ic;
use crate::theme::Palette;

/// Accent for a menu entry: neutral by default, colored like the danger entries when the
/// action carries meaning worth seeing at a glance (start = green, restart = amber).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuTone {
    #[default]
    Default,
    Danger,
    Success,
}

pub struct MenuItem {
    pub icon: Option<Ic>,
    pub label: SharedString,
    pub tone: MenuTone,
    pub on_click: Cb,
}

fn tone_color(p: &Palette, tone: MenuTone) -> Hsla {
    match tone {
        MenuTone::Danger if p.dark => p.danger,
        MenuTone::Danger => p.danger_text,
        MenuTone::Success => p.success,
        MenuTone::Default => p.text,
    }
}

fn popup_row(id: ElementId, p: &Palette, item: MenuItem, on_close: Cb) -> Stateful<Div> {
    let fg = tone_color(p, item.tone);
    let icon_color = if item.tone == MenuTone::Default { p.muted } else { fg };
    let on_click = item.on_click;
    menu_row(id, p)
        .on_click(move |_, w, cx| {
            on_close(w, cx);
            on_click(w, cx);
        })
        .children(item.icon.map(|ic| icon(ic, 16., icon_color)))
        .child(text(item.label, 13., FontWeight::NORMAL, fg).line_height(relative(1.)))
}

/// Floating menu under its (relative) parent, right-aligned, drawn above everything else.
pub fn popup_menu(id: impl Into<ElementId>, p: &Palette, items: Vec<MenuItem>, on_close: Cb) -> Div {
    let id: ElementId = id.into();
    let mut panel = menu_panel(id.clone(), p, on_close.clone()).w(px(200.));
    for (i, item) in items.into_iter().enumerate() {
        let item_id = ElementId::from((id.clone(), SharedString::from(format!("m{i}"))));
        panel = panel.child(popup_row(item_id, p, item, on_close.clone()));
    }
    float(ElementId::from((id, "popup")), CONTROL_H + 8., panel)
}

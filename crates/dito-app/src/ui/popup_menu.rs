use gpui_kit::prelude::*;
use gpui_kit::*;

use super::button::CONTROL_H;
use super::callbacks::Cb;
use super::menu_panel::{float, menu_label, menu_panel, menu_row, menu_separator};
use super::primitives::icon;
use super::typography::text;
use crate::theme::Palette;
use crate::ui::Ic;

/// For menus outside Local Models.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuTone {
    #[default]
    Default,
    Danger,
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
        MenuTone::Default => p.text,
    }
}

fn popup_row(id: ElementId, p: &Palette, item: MenuItem, on_close: Cb, neutral: bool) -> Stateful<Div> {
    let fg = tone_color(p, item.tone);
    let icon_color = if !neutral && item.tone == MenuTone::Default { p.muted } else { fg };
    let on_click = item.on_click;
    menu_row(id, p)
        .on_click(move |_, w, cx| {
            on_close(w, cx);
            on_click(w, cx);
        })
        .children(item.icon.map(|ic| icon(ic, 16., icon_color)))
        .child(text(item.label, 13., FontWeight::NORMAL, fg).line_height(relative(1.)))
}

pub fn popup_menu(id: impl Into<ElementId>, p: &Palette, items: Vec<MenuItem>, on_close: Cb) -> Div {
    let id: ElementId = id.into();
    let mut panel = menu_panel(id.clone(), p, on_close.clone()).w(px(200.));
    for (i, item) in items.into_iter().enumerate() {
        let item_id = ElementId::from((id.clone(), SharedString::from(format!("m{i}"))));
        panel = panel.child(popup_row(item_id, p, item, on_close.clone(), false));
    }
    float(ElementId::from((id, "popup")), CONTROL_H + 8., panel)
}

/// The menu closes before `on_click` runs.
pub struct AddEntry {
    pub label: SharedString,
    pub on_click: Cb,
}

fn add_row(id: ElementId, p: &Palette, entry: AddEntry, on_close: Cb) -> Stateful<Div> {
    let on_click = entry.on_click;
    menu_row(id, p)
        .on_click(move |_, w, cx| {
            on_close(w, cx);
            on_click(w, cx);
        })
        .child(text(entry.label, 14., FontWeight::NORMAL, p.text).line_height(px(20.)))
}

/// The caption says the entries are templates that only pre-fill the new configuration, not things to choose between
/// (`design:settings/cloud-models/add-from-template`).
pub fn add_menu(
    id: impl Into<ElementId>,
    p: &Palette,
    caption: impl Into<SharedString>,
    templates: Vec<AddEntry>,
    duplicate: AddEntry,
    on_close: Cb,
) -> Div {
    let id: ElementId = id.into();
    let mut panel = menu_panel(id.clone(), p, on_close.clone()).w(px(240.)).child(menu_label(p, caption));
    for (index, entry) in templates.into_iter().enumerate() {
        let entry_id = ElementId::from((id.clone(), SharedString::from(format!("template-{index}"))));
        panel = panel.child(add_row(entry_id, p, entry, on_close.clone()));
    }
    let duplicate_id = ElementId::from((id.clone(), "duplicate"));
    panel = panel.child(menu_separator(p)).child(add_row(duplicate_id, p, duplicate, on_close));
    float(ElementId::from((id, "popup")), CONTROL_H + 8., panel)
}

pub fn popup_menu_grouped(id: impl Into<ElementId>, p: &Palette, groups: Vec<Vec<MenuItem>>, on_close: Cb) -> Div {
    let id = id.into();
    let mut panel = menu_panel(id.clone(), p, on_close.clone()).w(px(224.));
    for (group, items) in groups.into_iter().filter(|items| !items.is_empty()).enumerate() {
        if group > 0 {
            panel = panel.child(menu_separator(p));
        }
        for (index, item) in items.into_iter().enumerate() {
            let item_id = ElementId::from((id.clone(), SharedString::from(format!("g{group}-m{index}"))));
            panel = panel.child(popup_row(item_id, p, item, on_close.clone(), true));
        }
    }
    float(ElementId::from((id, "popup")), CONTROL_H + 8., panel)
}

//! The cloud speech configurations as tiles, the menu of the plus button that adds one, and what the tiles, the menu and
//! the trash button do to the saved configurations.

use dito_core::config::CloudAsrTemplate;
use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::Value;

use super::AsrEditor;
use crate::settings::Ctx;
use crate::settings::asr_editor::configuration_list;
use crate::ui::{self, DialogAnswer, cbk, text};

/// The configuration a delete dialog asks about.
pub(super) struct ConfigurationDeletion {
    id: String,
    pub(super) name: String,
}

/// The model a configuration's request JSON names, or that it names none yet.
fn model_label(config: &Value, c: &Ctx) -> SharedString {
    match config["connection"]["model"].as_str().map(str::trim) {
        Some(model) if !model.is_empty() => model.to_owned().into(),
        _ => c.t(Tr::ModelsNoModel),
    }
}

impl AsrEditor {
    /// One tile per configuration: its name over the model its request JSON names. A click selects, a drag reorders,
    /// and the plus button opens the menu that adds one.
    pub(super) fn tile_row(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        let shown = list.active().map(|configuration| configuration.id.as_str());
        let tiles = list
            .items
            .iter()
            .map(|configuration| ui::ConfigTile {
                id: configuration.id.clone().into(),
                name: configuration.name.clone().into(),
                detail: model_label(&configuration.config, c),
                selected: Some(configuration.id.as_str()) == shown,
            })
            .collect();
        ui::tile_row("asr-tiles", &c.p, &self.tiles, tiles)
            .on_select(ui::cbk_id(cx, |this: &mut Self, id, window, cx| this.select_configuration(id, window, cx)))
            .on_move(ui::cbk_move(cx, |this: &mut Self, id, to, _, cx| this.move_configuration(id, to, cx)))
            .plus(ui::TilePlus {
                open: self.menu,
                on_press: cbk(cx, |this: &mut Self, _, cx| {
                    this.menu = !this.menu;
                    cx.notify();
                }),
                menu: self.menu.then(|| self.add_menu(c, cx)),
            })
            .into_any_element()
    }

    /// The bundled templates a new configuration can start from, then a copy of the selected one.
    fn add_menu(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let templates = CloudAsrTemplate::ALL
            .iter()
            .map(|&template| ui::AddEntry {
                label: template.name.into(),
                on_click: cbk(cx, move |this: &mut Self, window, cx| this.add_from_template(template, window, cx)),
            })
            .collect();
        let duplicate = ui::AddEntry {
            label: c.t(Tr::ModelsDuplicateConfiguration),
            on_click: cbk(cx, |this: &mut Self, window, cx| this.duplicate_shown_configuration(window, cx)),
        };
        let close = cbk(cx, |this: &mut Self, _, cx| {
            this.menu = false;
            cx.notify();
        });
        ui::add_menu("asr-add-menu", &c.p, c.t(Tr::ModelsCreateFromTemplate), templates, duplicate, close)
            .into_any_element()
    }

    /// A click on a tile: its configuration becomes the one in use, at once.
    fn select_configuration(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.saved(configuration_list::select(&self.st.cfg, id), cx).is_some() {
            self.sync(window, cx);
        }
    }

    /// A tile was dropped in a new place.
    fn move_configuration(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        self.saved(configuration_list::move_to(&self.st.cfg, id, to), cx);
        cx.notify();
    }

    fn new_configuration_name(&self) -> String {
        self.st.cfg.get().general.lang().tr(Tr::ModelsNewConfiguration).to_owned()
    }

    /// Add a configuration that starts from `template`.
    fn add_from_template(&mut self, template: CloudAsrTemplate, window: &mut Window, cx: &mut Context<Self>) {
        let added = configuration_list::add(&self.st.cfg, template, &self.new_configuration_name());
        if let Some(id) = self.saved(added, cx) {
            self.configuration_added(&id, window, cx);
        }
    }

    /// Add a copy of the configuration the form shows, as it is typed and without its key.
    fn duplicate_shown_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.shown.clone();
        self.flush(&id, cx);
        let copy = configuration_list::duplicate(&self.st.cfg, &id, &self.new_configuration_name());
        if let Some(Some(copy)) = self.saved(copy, cx) {
            self.configuration_added(&copy, window, cx);
        }
    }

    /// A configuration was added and is selected: show it, bring its tile into view and let the user name it.
    fn configuration_added(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = false;
        self.show(id, window, cx);
        self.name.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// The trash was pressed: ask whether to delete the configuration the form shows. The last one cannot go.
    pub(super) fn ask_delete_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        if list.items.len() < 2 {
            return;
        }
        let Some(shown) = list.items.iter().find(|configuration| configuration.id == self.shown) else { return };
        self.deleting = Some(ConfigurationDeletion { id: shown.id.clone(), name: shown.name.clone() });
        self.delete_focus.focus(window, cx);
        cx.emit(super::Changed);
        cx.notify();
    }

    fn close_delete_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.deleting = None;
        window.blur(cx);
        cx.emit(super::Changed);
        cx.notify();
    }

    /// Delete was confirmed.
    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ConfigurationDeletion { id, .. }) = self.deleting.take() else { return };
        window.blur(cx);
        self.delete_configuration(&id, window, cx);
        cx.emit(super::Changed);
    }

    /// The dialog that asks whether to delete a configuration (`20.05.12`), while it is open.
    pub(super) fn delete_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let name = self.deleting.as_ref()?.name.clone();
        let c = Ctx::new(&self.st, cx);
        let body =
            text(dito_core::i18n::fmt::delete_configuration_body(c.lang, &name), 13., FontWeight::NORMAL, c.p.text)
                .line_height(px(20.))
                .into_any_element();
        Some(ui::confirm_dialog(
            window,
            &c.p,
            "asr-delete-dialog",
            &self.delete_focus,
            c.t(Tr::ModelsDeleteConfigurationTitle),
            vec![body],
            DialogAnswer {
                label: c.t(Tr::AsrEditorCancel),
                on_click: Some(cbk(cx, |this: &mut Self, window, cx| this.close_delete_dialog(window, cx))),
            },
            DialogAnswer {
                label: c.t(Tr::PromptsRemoveConfirm),
                on_click: Some(cbk(cx, |this: &mut Self, window, cx| this.confirm_delete(window, cx))),
            },
        ))
    }

    /// Delete the configuration `id` with its key; its neighbour takes over. This is the one place a configuration is
    /// deleted: the confirmation of the dialog calls it, and so does anything that has to ask first.
    pub(super) fn delete_configuration(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let removed = configuration_list::remove(&self.st.cfg, &*self.st.secrets, id);
        if let Some(true) = self.saved(removed, cx) {
            self.drafts.remove(id);
            self.st.factory.invalidate_engines();
            self.sync(window, cx);
        }
    }
}

//! The cloud speech configurations as tiles, and what the tiles, the menu of the plus button and the trash button do to
//! the saved configurations. The row, the menu and the delete dialog are the shared [`ConfigurationPanel`]'s, as on the
//! cloud polish tab.

use dito_core::config::CloudAsrTemplate;
use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::Value;

use super::{AsrEditor, Changed};
use crate::settings::Ctx;
use crate::settings::asr_editor::configuration_list;
use crate::settings::configuration_panel::{
    ConfigurationHost, ConfigurationPanel, Deletion, RowActions, new_configuration_name,
};
use crate::ui::{self, cbk};

/// The model a configuration's request JSON names, or that it names none yet.
fn model_label(config: &Value, c: &Ctx) -> SharedString {
    match config["connection"]["model"].as_str().map(str::trim) {
        Some(model) if !model.is_empty() => model.to_owned().into(),
        _ => c.t(Tr::ModelsNoModel),
    }
}

impl ConfigurationHost for AsrEditor {
    fn panel(&mut self) -> &mut ConfigurationPanel {
        &mut self.tiles
    }

    fn dialog_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(Changed);
        cx.notify();
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
        self.tiles.tile_row(
            "asr-tiles",
            c,
            cx,
            tiles,
            RowActions {
                select: Self::select_configuration,
                move_to: |this, id, to, _, cx| this.move_configuration(id, to, cx),
                duplicate: Self::duplicate_shown_configuration,
                templates: Self::template_entries,
            },
        )
    }

    /// The rows of the plus menu that start a configuration from a bundled template.
    fn template_entries(_: &Ctx, cx: &mut Context<Self>) -> Vec<ui::AddEntry> {
        CloudAsrTemplate::ALL
            .iter()
            .map(|&template| ui::AddEntry {
                label: template.name.into(),
                on_click: cbk(cx, move |this: &mut Self, window, cx| this.add_from_template(template, window, cx)),
            })
            .collect()
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

    /// Add a configuration that starts from `template`.
    fn add_from_template(&mut self, template: CloudAsrTemplate, window: &mut Window, cx: &mut Context<Self>) {
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let added = configuration_list::add(&self.st.cfg, template, name);
        if let Some(id) = self.saved(added, cx) {
            self.configuration_added(&id, window, cx);
        }
    }

    /// Add a copy of the configuration the form shows, as it is typed and without its key.
    fn duplicate_shown_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.shown.clone();
        self.flush(&id, cx);
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let copy = configuration_list::duplicate(&self.st.cfg, &id, name);
        if let Some(Some(copy)) = self.saved(copy, cx) {
            self.configuration_added(&copy, window, cx);
        }
    }

    /// A configuration was added and is selected: the form shows it, and the user names it.
    fn configuration_added(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show(id, window, cx);
        self.tiles.added(id, &self.name, window, cx);
    }

    /// The trash was pressed: ask whether to delete the configuration the form shows. The last one cannot go.
    pub(super) fn ask_delete_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.st.cfg.get();
        let list = configuration_list::of(&config);
        if list.items.len() < 2 {
            return;
        }
        let Some(shown) = list.items.iter().find(|configuration| configuration.id == self.shown) else { return };
        let deletion = Deletion { id: shown.id.clone(), name: shown.name.clone() };
        ConfigurationPanel::ask_delete(self, deletion, window, cx);
    }

    /// The dialog that asks whether to delete a configuration (`design:models/cloud/delete-configuration`), while it is open.
    pub(super) fn delete_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.tiles.deleting()?;
        let c = Ctx::new(&self.st, cx);
        self.tiles.delete_dialog(window, &c, cx, Self::delete_configuration)
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

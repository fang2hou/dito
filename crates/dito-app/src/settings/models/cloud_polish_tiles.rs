//! The cloud polish configurations as tiles (20.05.1, 20.05.7 to 20.05.13): what feeds the row, and what the tiles, the
//! menu of the plus button and the trash do to the saved configurations. The row, the menu and the delete dialog are
//! the shared [`ConfigurationPanel`]'s, as on the cloud speech tab.

use dito_core::config::{CloudPolishConfigurations, CloudPolishTemplate};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::{ModalChanged, ModelsPage};
use super::polish_field_rules::CloudPolishField;
use crate::services;
use crate::settings::Ctx;
use crate::settings::configuration_panel::{
    ConfigurationHost, ConfigurationPanel, Deletion, RowActions, new_configuration_name,
};
use crate::ui::{self, TestState, cbk};

/// What the tile row shows of each cloud polish configuration, in the order of the list: the name over the model, or
/// over the note that it has none yet.
pub(super) fn cloud_polish_config_tiles(configurations: &CloudPolishConfigurations, lang: Lang) -> Vec<ui::ConfigTile> {
    configurations
        .items
        .iter()
        .map(|item| {
            let model = item.target.model().trim();
            ui::ConfigTile {
                id: item.id.clone().into(),
                name: item.name.clone().into(),
                detail: if model.is_empty() { lang.tr(Tr::ModelsNoModel).into() } else { model.to_owned().into() },
                selected: item.id == configurations.selected,
            }
        })
        .collect()
}

/// The page hosts the cloud polish tiles; the cloud speech tiles belong to its editor.
impl ConfigurationHost for ModelsPage {
    fn panel(&mut self) -> &mut ConfigurationPanel {
        &mut self.cloud_polish.tiles
    }

    fn dialog_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(ModalChanged);
        cx.notify();
    }
}

impl ModelsPage {
    /// One tile per saved cloud polish configuration, and the plus button that adds one. A click selects, and the form
    /// follows; a drag reorders; the order is saved when the tile is dropped.
    pub(super) fn cloud_polish_tile_row(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let cfg = self.st.cfg.get();
        let tiles = cloud_polish_config_tiles(&cfg.polish.engine.polish_configurations(c.lang), c.lang);
        self.cloud_polish.tiles.tile_row(
            "cloud-polish-tiles",
            c,
            cx,
            tiles,
            RowActions {
                select: |this, id, _, cx| this.select_cloud_polish(id, cx),
                move_to: |this, id, to, _, cx| this.move_cloud_polish(id, to, cx),
                duplicate: Self::duplicate_cloud_polish,
                templates: Self::cloud_polish_templates,
            },
        )
    }

    /// The rows of the plus menu that start a configuration from a template: the three API formats.
    fn cloud_polish_templates(c: &Ctx, cx: &mut Context<Self>) -> Vec<ui::AddEntry> {
        CloudPolishTemplate::ALL
            .iter()
            .map(|&template| ui::AddEntry {
                label: c.t(template.label()),
                on_click: cbk(cx, move |this: &mut Self, window, cx| {
                    this.add_cloud_polish_from_template(template, window, cx)
                }),
            })
            .collect()
    }

    /// Make `id` the cloud polish configuration in use. The form follows the selection when the page is drawn.
    fn select_cloud_polish(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.cloud_polish.form.shown() == Some(id) {
            return;
        }
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            cfg.polish.engine.polish_configurations_mut(lang).select(id);
        });
        self.note_save(saved.is_ok());
        self.cloud_polish.test = TestState::Idle;
        cx.notify();
    }

    /// A tile was dropped in a new place: the order is saved.
    fn move_cloud_polish(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            cfg.polish.engine.polish_configurations_mut(lang).move_to(id, to);
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    /// Add a cloud polish configuration that starts from `template`, named `新しい設定` in the display language.
    fn add_cloud_polish_from_template(
        &mut self,
        template: CloudPolishTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let mut added = None;
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            added = Some(cfg.polish.engine.polish_configurations_mut(lang).add_from_template(template, name));
        });
        self.note_save(saved.is_ok());
        match added.filter(|_| saved.is_ok()) {
            Some(id) => self.cloud_polish_added(&id, window, cx),
            None => cx.notify(),
        }
    }

    /// Add a copy of the cloud polish configuration the form shows, without its key.
    fn duplicate_cloud_polish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.cloud_polish.form.shown().map(str::to_owned) else { return };
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let mut copy = None;
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            copy = cfg.polish.engine.polish_configurations_mut(lang).duplicate(&source, name);
        });
        self.note_save(saved.is_ok());
        match copy.filter(|_| saved.is_ok()) {
            Some(id) => self.cloud_polish_added(&id, window, cx),
            None => cx.notify(),
        }
    }

    /// The configuration `id` was added and is selected: the form shows it, and the user names it.
    fn cloud_polish_added(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.cloud_polish.test = TestState::Idle;
        let lang = self.st.cfg.get().general.lang();
        self.follow_polish_settings(lang, window, cx);
        self.cloud_polish.tiles.added(id, &self.cloud_polish.form[CloudPolishField::Name], window, cx);
        cx.notify();
    }

    /// The trash was pressed: ask whether to delete the cloud polish configuration in use. The last one cannot go.
    pub(super) fn ask_delete_cloud_polish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let configurations = cfg.polish.engine.polish_configurations(cfg.general.lang());
        let Some(active) = configurations.active().filter(|_| configurations.items.len() > 1) else { return };
        let deletion = Deletion { id: active.id.clone(), name: active.name.clone() };
        ConfigurationPanel::ask_delete(self, deletion, window, cx);
    }

    /// Delete was confirmed: the configuration and its Keychain key go together, and its neighbour is selected.
    pub(super) fn delete_cloud_polish(&mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let removed = self.st.cfg.remove_cloud_polish_configuration(id, &*self.st.secrets);
        if let Err(error) = &removed {
            tracing::warn!("could not delete a cloud polish configuration: {error:#}");
        }
        // An engine built from the deleted configuration must not keep its key.
        self.st.factory.invalidate_engines();
        self.note_save(removed.is_ok());
        self.cloud_polish.test = TestState::Idle;
        cx.notify();
    }

    /// The dialog that asks whether to delete the cloud polish configuration in use (20.05.12), while it is open.
    pub(super) fn cloud_polish_delete_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.cloud_polish.tiles.deleting()?;
        let c = Ctx::new(&self.st, cx);
        self.cloud_polish.tiles.delete_dialog(window, &c, cx, Self::delete_cloud_polish)
    }
}

#[cfg(test)]
mod tests {
    use dito_core::config::CloudPolishConfigurations;
    use dito_core::i18n::{Lang, Tr};

    use super::{CloudPolishField, cloud_polish_config_tiles};

    #[test]
    fn the_tile_feed_follows_the_name_and_model_as_they_are_typed() {
        let mut list = CloudPolishConfigurations::new(Lang::En);
        list.select("compat");
        let compat = list.get_mut("compat").unwrap();
        assert!(CloudPolishField::Name.apply(compat, "Fast"));
        assert!(CloudPolishField::Model.apply(compat, "gpt-x"));
        let feed: Vec<_> = cloud_polish_config_tiles(&list, Lang::En)
            .into_iter()
            .map(|t| (t.id.to_string(), t.name.to_string(), t.detail.to_string(), t.selected))
            .collect();
        let none = Lang::En.tr(Tr::ModelsNoModel);
        assert_eq!(
            feed,
            [
                ("azure".into(), "Azure OpenAI".into(), none.into(), false),
                ("bedrock".into(), "Amazon Bedrock".into(), none.into(), false),
                ("compat".into(), "Fast".into(), "gpt-x".into(), true),
            ],
            "ordered as saved, the selected one marked, a missing model said so"
        );
    }

    #[test]
    fn a_model_of_blanks_counts_as_none() {
        let mut list = CloudPolishConfigurations::new(Lang::Ja);
        assert!(CloudPolishField::Model.apply(list.get_mut("azure").unwrap(), "   "));
        let feed = cloud_polish_config_tiles(&list, Lang::Ja);
        assert_eq!(feed[0].detail.to_string(), Lang::Ja.tr(Tr::ModelsNoModel));
    }
}

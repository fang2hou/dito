//! The cloud polish tile row, on the shared [`ConfigurationPanel`] (`design:settings/cloud-models/speech-form`,
//! `design:settings/cloud-models/configurations`, `design:settings/cloud-models/add-from-template`,
//! `design:settings/cloud-models/delete-configuration`, `design:settings/cloud-models/tile-row`).

use dito_core::config::{CloudTextConfigurations, CloudTextTemplate};
use dito_core::i18n::{Lang, Tr};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::{ModalChanged, ModelsPage};
use super::text_field_rules::CloudTextField;
use crate::services;
use crate::settings::Ctx;
use crate::settings::configuration_panel::{
    ConfigurationHost, ConfigurationPanel, Deletion, RowActions, new_configuration_name,
};
use crate::ui::{self, TestState, cbk};

pub(super) fn cloud_text_config_tiles(configurations: &CloudTextConfigurations, lang: Lang) -> Vec<ui::ConfigTile> {
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

impl ConfigurationHost for ModelsPage {
    fn panel(&mut self) -> &mut ConfigurationPanel {
        &mut self.cloud_text.tiles
    }

    fn dialog_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(ModalChanged);
        cx.notify();
    }
}

impl ModelsPage {
    pub(super) fn cloud_text_tile_row(&self, c: &Ctx, cx: &mut Context<Self>) -> AnyElement {
        let cfg = self.st.cfg.get();
        let tiles = cloud_text_config_tiles(&cfg.text_model.text_configurations(c.lang), c.lang);
        self.cloud_text.tiles.tile_row(
            "cloud-polish-tiles",
            c,
            cx,
            tiles,
            RowActions {
                select: |this, id, _, cx| this.select_cloud_text(id, cx),
                move_to: |this, id, to, _, cx| this.move_cloud_text(id, to, cx),
                duplicate: Self::duplicate_cloud_text,
                templates: Self::cloud_text_templates,
            },
        )
    }

    fn cloud_text_templates(c: &Ctx, cx: &mut Context<Self>) -> Vec<ui::AddEntry> {
        CloudTextTemplate::ALL
            .iter()
            .map(|&template| ui::AddEntry {
                label: c.t(template.label()),
                on_click: cbk(cx, move |this: &mut Self, window, cx| {
                    this.add_cloud_text_from_template(template, window, cx)
                }),
            })
            .collect()
    }

    /// The form follows the selection when the page is drawn.
    fn select_cloud_text(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.cloud_text.form.shown() == Some(id) {
            return;
        }
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            cfg.text_model.text_configurations_mut(lang).select(id);
        });
        self.note_save(saved.is_ok());
        self.cloud_text.test = TestState::Idle;
        cx.notify();
    }

    fn move_cloud_text(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            cfg.text_model.text_configurations_mut(lang).move_to(id, to);
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    /// Named in the display language.
    fn add_cloud_text_from_template(
        &mut self,
        template: CloudTextTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let mut added = None;
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            added = Some(cfg.text_model.text_configurations_mut(lang).add_from_template(template, name));
        });
        self.note_save(saved.is_ok());
        match added.filter(|_| saved.is_ok()) {
            Some(id) => self.cloud_text_added(&id, window, cx),
            None => cx.notify(),
        }
    }

    /// The copy does not get the key.
    fn duplicate_cloud_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.cloud_text.form.shown().map(str::to_owned) else { return };
        let name = new_configuration_name(self.st.cfg.get().general.lang());
        let mut copy = None;
        let saved = services::try_save_config(&self.st, |cfg| {
            let lang = cfg.general.lang();
            copy = cfg.text_model.text_configurations_mut(lang).duplicate(&source, name);
        });
        self.note_save(saved.is_ok());
        match copy.filter(|_| saved.is_ok()) {
            Some(id) => self.cloud_text_added(&id, window, cx),
            None => cx.notify(),
        }
    }

    fn cloud_text_added(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.cloud_text.test = TestState::Idle;
        let lang = self.st.cfg.get().general.lang();
        self.follow_text_model_settings(lang, window, cx);
        self.cloud_text.tiles.added(id, &self.cloud_text.form[CloudTextField::Name], window, cx);
        cx.notify();
    }

    /// The last configuration cannot be deleted.
    pub(super) fn ask_delete_cloud_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.st.cfg.get();
        let configurations = cfg.text_model.text_configurations(cfg.general.lang());
        let Some(active) = configurations.active().filter(|_| configurations.items.len() > 1) else { return };
        let deletion = Deletion { id: active.id.clone(), name: active.name.clone() };
        ConfigurationPanel::ask_delete(self, deletion, window, cx);
    }

    /// The configuration and its Keychain key go together.
    pub(super) fn delete_cloud_text(&mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let removed = self.st.cfg.remove_cloud_text_configuration(id, &*self.st.secrets);
        if let Err(error) = &removed {
            tracing::warn!("could not delete a cloud polish configuration: {error:#}");
        }
        // An engine built from the deleted configuration must not keep its key.
        self.st.factory.invalidate_engines();
        self.note_save(removed.is_ok());
        self.cloud_text.test = TestState::Idle;
        cx.notify();
    }

    /// `design:settings/cloud-models/delete-configuration`.
    pub(super) fn cloud_text_delete_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.cloud_text.tiles.deleting()?;
        let c = Ctx::new(&self.st, cx);
        self.cloud_text.tiles.delete_dialog(window, &c, cx, Self::delete_cloud_text)
    }
}

#[cfg(test)]
mod tests {
    use dito_core::config::CloudTextConfigurations;
    use dito_core::i18n::{Lang, Tr};

    use super::{CloudTextField, cloud_text_config_tiles};

    #[test]
    fn the_tile_feed_follows_the_name_and_model_as_they_are_typed() {
        let mut list = CloudTextConfigurations::new(Lang::En);
        list.select("compat");
        let compat = list.get_mut("compat").unwrap();
        assert!(CloudTextField::Name.apply(compat, "Fast"));
        assert!(CloudTextField::Model.apply(compat, "gpt-x"));
        let feed: Vec<_> = cloud_text_config_tiles(&list, Lang::En)
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
        let mut list = CloudTextConfigurations::new(Lang::Ja);
        assert!(CloudTextField::Model.apply(list.get_mut("azure").unwrap(), "   "));
        let feed = cloud_text_config_tiles(&list, Lang::Ja);
        assert_eq!(feed[0].detail.to_string(), Lang::Ja.tr(Tr::ModelsNoModel));
    }
}

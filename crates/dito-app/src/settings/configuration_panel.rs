//! What the cloud speech form and the cloud polish form do the same way with their configurations (`design:models/cloud/speech-form`, `design:models/cloud/add-from-template`,
//! `design:models/cloud/delete-configuration`): a row of tiles with the menu of its plus button, the dialog that asks before one is deleted, and the step
//! after one was added. The owner keeps what differs: the tiles it feeds, the templates it offers, and what selecting,
//! moving, duplicating and deleting do to its own settings.

use dito_core::i18n::{Lang, Tr, fmt};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Ctx;
use crate::ui::{self, AddEntry, ConfigTile, DialogAnswer, InputState, TilePlus, TileRowState, cbk, text};

/// The id of the delete dialog. One is open at a time, whichever form it belongs to.
const DELETE_DIALOG: &str = "configuration-delete-dialog";

/// What a new configuration is named, whether it starts from a template or copies another one.
pub(in crate::settings) fn new_configuration_name(lang: Lang) -> &'static str {
    lang.tr(Tr::ModelsNewConfiguration)
}

/// The configuration a delete dialog asks about.
pub(in crate::settings) struct Deletion {
    pub(in crate::settings) id: String,
    pub(in crate::settings) name: String,
}

/// A view that keeps a [`ConfigurationPanel`].
pub(in crate::settings) trait ConfigurationHost: Sized + 'static {
    fn panel(&mut self) -> &mut ConfigurationPanel;

    /// A delete dialog opened or closed. The window shell paints it over everything else, so it has to be told.
    fn dialog_changed(&mut self, cx: &mut Context<Self>);
}

/// What the owner provides for the row: what a click on a tile, a tile dropped in a new place and the copy row of the
/// plus menu do, and the templates that menu offers.
pub(in crate::settings) struct RowActions<V: 'static> {
    pub(in crate::settings) select: fn(&mut V, &str, &mut Window, &mut Context<V>),
    pub(in crate::settings) move_to: fn(&mut V, &str, usize, &mut Window, &mut Context<V>),
    /// Copy the configuration in use, as the owner means it.
    pub(in crate::settings) duplicate: fn(&mut V, &mut Window, &mut Context<V>),
    /// The rows of the menu that start a new configuration from a template, built when the menu is open.
    pub(in crate::settings) templates: fn(&Ctx, &mut Context<V>) -> Vec<AddEntry>,
}

/// The state of one list of configurations on screen: where its tile row is scrolled, whether the menu of the plus
/// button is open, and the configuration a delete dialog asks about.
pub(in crate::settings) struct ConfigurationPanel {
    tiles: TileRowState,
    menu_open: bool,
    deleting: Option<Deletion>,
    delete_focus: FocusHandle,
}

impl ConfigurationPanel {
    pub(in crate::settings) fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        Self { tiles: TileRowState::new(), menu_open: false, deleting: None, delete_focus: cx.focus_handle() }
    }

    /// Whether the menu of the plus button is open.
    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn menu_open(&self) -> bool {
        self.menu_open
    }

    /// The configuration the open delete dialog asks about.
    pub(in crate::settings) fn deleting(&self) -> Option<&Deletion> {
        self.deleting.as_ref()
    }

    pub(in crate::settings) fn close_menu(&mut self) {
        self.menu_open = false;
    }

    /// A dialog and a menu belong to the visit that opened them.
    pub(in crate::settings) fn reset(&mut self) {
        self.menu_open = false;
        self.deleting = None;
    }

    /// Scroll the tile `id` fully into view the next time the row draws, after the selection moved to it.
    pub(in crate::settings) fn reveal(&self, id: &str) {
        self.tiles.reveal(id.to_owned());
    }

    /// The scroll offset of the row, the furthest it goes and the slot a dragged tile is over.
    #[cfg(feature = "screenshots")]
    pub(in crate::settings) fn probe(&self) -> (f32, f32, Option<usize>) {
        self.tiles.probe()
    }

    /// The row of `tiles` and the plus button that ends it, `id` naming the row. A click selects, a drag reorders and
    /// the plus button opens the menu that adds a configuration.
    pub(in crate::settings) fn tile_row<V: ConfigurationHost>(
        &self,
        id: &'static str,
        c: &Ctx,
        cx: &mut Context<V>,
        tiles: Vec<ConfigTile>,
        actions: RowActions<V>,
    ) -> AnyElement {
        let RowActions { select, move_to, duplicate, templates } = actions;
        let menu = self.menu_open.then(|| {
            let duplicate = AddEntry {
                label: c.t(Tr::ModelsDuplicateConfiguration),
                on_click: cbk(cx, move |this: &mut V, window, cx| duplicate(this, window, cx)),
            };
            let close = cbk(cx, |this: &mut V, _, cx| {
                this.panel().menu_open = false;
                cx.notify();
            });
            ui::add_menu(
                ElementId::from((ElementId::from(id), "add-menu")),
                &c.p,
                c.t(Tr::ModelsCreateFromTemplate),
                templates(c, cx),
                duplicate,
                close,
            )
            .into_any_element()
        });
        ui::tile_row(id, &c.p, &self.tiles, tiles)
            .on_select(ui::cbk_id(cx, move |this: &mut V, tile, window, cx| select(this, tile, window, cx)))
            .on_move(ui::cbk_move(cx, move |this: &mut V, tile, to, window, cx| move_to(this, tile, to, window, cx)))
            .plus(TilePlus {
                open: self.menu_open,
                on_press: cbk(cx, |this: &mut V, _, cx| {
                    let panel = this.panel();
                    panel.menu_open = !panel.menu_open;
                    cx.notify();
                }),
                menu,
            })
            .into_any_element()
    }

    /// A configuration was added and is selected, and the owner's form shows it: the menu closes, its tile scrolls into
    /// view, and the `name` field is focused with all of its text selected, for the user to name it.
    pub(in crate::settings) fn added<V: 'static>(
        &mut self,
        id: &str,
        name: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        self.menu_open = false;
        self.reveal(id);
        name.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// The trash was pressed: ask whether to delete `deletion`.
    pub(in crate::settings) fn ask_delete<V: ConfigurationHost>(
        host: &mut V,
        deletion: Deletion,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        let panel = host.panel();
        panel.deleting = Some(deletion);
        panel.delete_focus.focus(window, cx);
        host.dialog_changed(cx);
    }

    /// The dialog that asks whether to delete a configuration (`design:models/cloud/delete-configuration`), while it is open. Delete runs `delete` on the
    /// owner, with the id of the configuration the dialog asked about.
    pub(in crate::settings) fn delete_dialog<V: ConfigurationHost>(
        &self,
        window: &Window,
        c: &Ctx,
        cx: &mut Context<V>,
        delete: fn(&mut V, &str, &mut Window, &mut Context<V>),
    ) -> Option<AnyElement> {
        let name = &self.deleting.as_ref()?.name;
        let body = text(fmt::delete_configuration_body(c.lang, name), 13., FontWeight::NORMAL, c.p.text)
            .line_height(px(20.))
            .into_any_element();
        Some(ui::confirm_dialog(
            window,
            &c.p,
            DELETE_DIALOG,
            &self.delete_focus,
            c.t(Tr::ModelsDeleteConfigurationTitle),
            vec![body],
            DialogAnswer {
                label: c.t(Tr::AsrEditorCancel),
                on_click: Some(cbk(cx, |this: &mut V, window, cx| close_delete(this, window, cx))),
            },
            DialogAnswer {
                label: c.t(Tr::PromptsRemoveConfirm),
                on_click: Some(cbk(cx, move |this: &mut V, window, cx| confirm_delete(this, delete, window, cx))),
            },
        ))
    }
}

/// Cancel, and Escape, closed the dialog: nothing was deleted.
fn close_delete<V: ConfigurationHost>(host: &mut V, window: &mut Window, cx: &mut Context<V>) {
    host.panel().deleting = None;
    window.blur(cx);
    host.dialog_changed(cx);
}

/// Delete was confirmed.
fn confirm_delete<V: ConfigurationHost>(
    host: &mut V,
    delete: fn(&mut V, &str, &mut Window, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(Deletion { id, .. }) = host.panel().deleting.take() else { return };
    window.blur(cx);
    delete(host, &id, window, cx);
    host.dialog_changed(cx);
}

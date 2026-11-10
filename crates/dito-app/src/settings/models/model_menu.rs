use std::rc::Rc;

use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::catalog::ModelSpec;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::state::AppState;
use crate::ui::{self, MenuItem, MenuTone, cbk};

impl ModelsPage {
    pub(super) fn model_menu(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        spec: &ModelSpec,
        selected: bool,
    ) -> Div {
        let close = cbk(cx, |this: &mut Self, _, cx| {
            this.menu = None;
            cx.notify();
        });
        let runtime = self.engine_command_items(c, cx, role, &spec.id, selected);
        let mut files = self.file_command_items(c, &spec.id);
        let removal = files.split_off(2);
        files.swap(0, 1);
        ui::popup_menu_grouped(
            SharedString::from(format!("menu-{}", spec.id)),
            &c.p,
            vec![runtime, files, removal],
            close,
        )
    }

    /// Engine commands follow the runtime: Stop + Restart while the engine runs, Start otherwise. Starting a
    /// non-selected model also switches the engine to it.
    fn engine_command_items(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        role: Role,
        id: &str,
        selected: bool,
    ) -> Vec<MenuItem> {
        let running = matches!(self.st.local.status(id), EngineStatus::Running | EngineStatus::Loading);
        let (st, id) = (self.st.clone(), id.to_string());
        if running && selected {
            let (stop_st, stop_id) = (st.clone(), id.clone());
            return vec![
                MenuItem {
                    icon: Some(Ic::Stop),
                    label: c.t(Tr::ModelsStop),
                    tone: MenuTone::Default,
                    on_click: Rc::new(move |_, _| {
                        let (local, id) = (stop_st.local.clone(), stop_id.clone());
                        stop_st.rt.spawn_blocking(move || local.unload(&id));
                    }),
                },
                MenuItem {
                    icon: Some(Ic::Refresh),
                    label: c.t(Tr::ModelsRestart),
                    tone: MenuTone::Default,
                    on_click: Rc::new(move |_, _| restart_engine(&st, &id)),
                },
            ];
        }
        vec![MenuItem {
            icon: Some(Ic::Play),
            label: c.t(Tr::ModelsStart),
            tone: MenuTone::Default,
            on_click: cbk(cx, move |this: &mut Self, _, cx| this.start_model(role, &id, cx)),
        }]
    }

    /// Start the model `id`, which also makes it the one `role` uses. A selection that cannot be saved starts nothing.
    fn start_model(&mut self, role: Role, id: &str, cx: &mut Context<Self>) {
        if self.select_local_model(role, id) {
            restart_engine(&self.st, id);
        }
        cx.notify();
    }

    /// Download again, show in Finder and delete.
    fn file_command_items(&self, c: &Ctx, id: &str) -> Vec<MenuItem> {
        let (st, again_id, delete_id) = (self.st.clone(), id.to_string(), id.to_string());
        let dir = self.st.models.model_dir(id);
        let again_st = st.clone();
        vec![
            MenuItem {
                icon: Some(Ic::Download),
                label: c.t(Tr::ModelsDownloadAgain),
                tone: MenuTone::Default,
                on_click: Rc::new(move |_, _| {
                    if again_st.models.delete(&again_id).is_ok() {
                        let _ = again_st.models.start_download(&again_id);
                    }
                }),
            },
            MenuItem {
                icon: Some(Ic::External),
                label: c.t(Tr::MenubarShowInFinder),
                tone: MenuTone::Default,
                on_click: Rc::new(move |_, _| {
                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                }),
            },
            MenuItem {
                icon: Some(Ic::Trash),
                label: c.t(Tr::ModelsDelete),
                tone: MenuTone::Default,
                on_click: Rc::new(move |_, _| {
                    let (local, models, id) = (st.local.clone(), st.models.clone(), delete_id.clone());
                    st.rt.spawn_blocking(move || {
                        local.unload(&id);
                        let _ = models.delete(&id);
                    });
                }),
            },
        ]
    }
}

fn restart_engine(st: &AppState, id: &str) {
    let (local, id) = (st.local.clone(), id.to_string());
    st.rt.spawn(async move {
        local.restart(&id).await;
    });
}

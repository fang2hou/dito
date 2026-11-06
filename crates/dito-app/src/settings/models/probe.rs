//! What the screenshot harness steers on the Models page.

use dito_core::secrets::Role;
use gpui_kit::*;

use crate::settings::asr_editor::EditorPreview;
use crate::ui;

use super::page::ModelsPage;

impl ModelsPage {
    pub(in crate::settings) fn preview(
        &mut self,
        role: Role,
        editor: Option<EditorPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = role;
        // Synthetic visual states never touch downloads or files on disk.
        if std::env::var_os("DITO_MODEL_STATES").is_some() {
            let kind = if role == Role::Asr { dito_local::ModelKind::Asr } else { dito_local::ModelKind::Polish };
            let specs: Vec<_> = self.st.models.catalog().iter().filter(|m| m.kind == kind).collect();
            let mut states = [
                dito_local::ModelState::Downloading {
                    done: std::env::var("DITO_MODEL_PROGRESS")
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .map(|f| (f.clamp(0., 1.) * 1_000_000_000.) as u64)
                        .unwrap_or(780_000_000),
                    total: 1_000_000_000,
                    bytes_per_sec: 11_000_000.,
                },
                dito_local::ModelState::Failed {
                    message: dito_core::i18n::Lang::Ja.tr(dito_core::i18n::Tr::ModelsConnectionLost).to_string(),
                    done: 120_000_000,
                    total: 240_000_000,
                },
                dito_local::ModelState::Installed,
            ];
            if std::env::var("DITO_MODEL_STATES").as_deref() == Ok("actions") {
                states[0] = dito_local::ModelState::Installed;
                states[1] = dito_local::ModelState::NotInstalled;
            }
            self.preview_snapshot = Some(std::sync::Arc::new(
                specs
                    .iter()
                    .enumerate()
                    .map(|(index, s)| {
                        let state = if index == specs.len() - 1 {
                            dito_local::ModelState::Installed
                        } else {
                            states.get(index).cloned().unwrap_or(dito_local::ModelState::NotInstalled)
                        };
                        (s.id.clone(), state)
                    })
                    .collect(),
            ));
            if let Some(spec) = specs.last() {
                let id = spec.id.clone();
                if std::env::var_os("DITO_MODEL_MENU").is_some() {
                    self.menu = Some(id.clone());
                }
                crate::services::save_config(&self.st, |cfg| {
                    crate::settings::role::engine_mut(cfg, role).local_model = id
                });
            }
            if let Some(spec) = specs.first() {
                ui::force_hover(&self.motion.handle(), SharedString::from(format!("pause-{}", spec.id)).into(), true);
            }
        }
        self.editor = Self::make_editor(self.st.clone(), window, cx);
        if let Some(preview) = editor {
            self.editor.update(cx, |view, cx| view.preview(preview, window, cx));
        }
        cx.notify();
    }

    /// The page's scope, for the screenshot harness.
    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

//! What the screenshot harness steers on the Models page.

use dito_core::secrets::Role;
use gpui_kit::*;

use crate::settings::asr_editor::EditorPreview;
use crate::ui::{self, InputEvent, InputState, TestState};

use super::page::{CloudTextPanel, ModelsPage, Tab};
use super::text_field_rules::{CloudTextField, LocalTextField};

impl ModelsPage {
    pub(in crate::settings) fn preview(
        &mut self,
        role: Role,
        editor: Option<EditorPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = Tab::Role(role);
        // Each shot starts from the settings alone: nothing typed, scrolled or opened before is left in the page.
        self.cloud_text = CloudTextPanel::new(window, cx);
        self.save_failed = false;
        // Synthetic visual states never touch downloads or files on disk.
        if std::env::var_os("DITO_MODEL_STATES").is_some() {
            let kind = if role == Role::Asr { dito_local::ModelKind::Asr } else { dito_local::ModelKind::Text };
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

    pub(in crate::settings) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }

    /// `arg` names the field, the configuration or the model; `text` is what is typed.
    pub(in crate::settings) fn probe(
        &mut self,
        case: &str,
        arg: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match case {
            "type" => {
                let field = CloudTextField::ALL.into_iter().find(|f| cloud_name(*f) == arg).expect("a field");
                replace_text(&self.cloud_text.form[field], text, window, cx);
            }
            "type-local" => {
                let field = LocalTextField::ALL.into_iter().find(|f| local_name(*f) == arg).expect("a field");
                replace_text(&self.local_text[field], text, window, cx);
            }
            // A window that is never shown moves no focus, so focus leaving the name is delivered as the input's own
            // event, which is what the page listens to.
            "blur-name" => self.cloud_text.form[CloudTextField::Name].update(cx, |_, cx| cx.emit(InputEvent::Blur)),
            "source" => {
                if let Tab::Role(role) = self.tab {
                    self.set_source(role, if arg == "local" { 0 } else { 1 }, cx);
                }
            }
            "use-model" => {
                if let Tab::Role(role) = self.tab {
                    self.use_model(role, arg);
                }
            }
            "vad" => self.show_vad(cx),
            // The form shows a configuration that was just deleted until the page is drawn again, and a key typed
            // into that gap has nowhere to go.
            "delete-then-key" => {
                let Some(id) = self.cloud_text.form.shown().map(str::to_owned) else { return };
                self.delete_cloud_text(&id, window, cx);
                self.cloud_text.form[CloudTextField::Key]
                    .update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
                self.cloud_text_edited(CloudTextField::Key, cx);
            }
            "local-states" => {
                let specs: Vec<_> = self
                    .st
                    .models
                    .catalog()
                    .iter()
                    .filter(|m| m.kind == dito_local::ModelKind::Text)
                    .map(|m| m.id.clone())
                    .collect();
                self.preview_snapshot = Some(std::sync::Arc::new(
                    specs
                        .iter()
                        .enumerate()
                        .map(|(index, id)| {
                            let state = if index < 2 {
                                dito_local::ModelState::Installed
                            } else {
                                dito_local::ModelState::NotInstalled
                            };
                            (id.clone(), state)
                        })
                        .collect(),
                ));
                if let Some(first) = specs.first().cloned() {
                    crate::services::save_config(&self.st, |cfg| cfg.text_model.local_model = first);
                }
                cx.notify();
            }
            other => panic!("unknown Models probe step {other:?}"),
        }
    }

    pub(in crate::settings) fn probe_state(&self, window: &Window, cx: &App) -> ModelsProbe {
        let cloud = &self.cloud_text.form;
        let local = &self.local_text;
        let name = cloud[CloudTextField::Name].read(cx);
        ModelsProbe {
            save_failed: self.save_failed,
            menu_open: self.cloud_text.tiles.menu_open(),
            name_focused: name.focus_handle(cx).is_focused(window),
            name_selection: name.selected_range(),
            tiles: self.cloud_text.tiles.probe(),
            test: match &self.cloud_text.test {
                TestState::Idle => "idle",
                TestState::Running => "running",
                TestState::Ok(_) => "ok",
                TestState::Failed(_) => "failed",
            },
            deleting: self.cloud_text.tiles.deleting().map(|deleting| deleting.name.clone()),
            shown: cloud.shown().map(str::to_owned),
            texts: CloudTextField::ALL.map(|field| (cloud_name(field), cloud.text(field, cx))).into(),
            invalid: CloudTextField::ALL.into_iter().filter(|f| cloud.invalid(*f, cx)).map(cloud_name).collect(),
            local_shown: local.shown().map(str::to_owned),
            local_texts: LocalTextField::ALL.map(|field| (local_name(field), local.text(field, cx))).into(),
            local_invalid: LocalTextField::ALL.into_iter().filter(|f| local.invalid(*f, cx)).map(local_name).collect(),
            local_models: self
                .st
                .models
                .catalog()
                .iter()
                .filter(|m| m.kind == dito_local::ModelKind::Text)
                .map(|m| m.id.clone())
                .collect(),
        }
    }
}

pub struct ModelsProbe {
    pub save_failed: bool,
    pub menu_open: bool,
    pub name_focused: bool,
    pub name_selection: std::ops::Range<usize>,
    pub tiles: (f32, f32, Option<usize>),
    /// `idle`, `running`, `ok` or `failed`: the connection test of the cloud polish form.
    pub test: &'static str,
    pub deleting: Option<String>,
    pub shown: Option<String>,
    pub texts: Vec<(&'static str, String)>,
    pub invalid: Vec<&'static str>,
    pub local_shown: Option<String>,
    pub local_texts: Vec<(&'static str, String)>,
    pub local_invalid: Vec<&'static str>,
    pub local_models: Vec<String>,
}

impl ModelsProbe {
    pub fn text(&self, field: &str) -> String {
        self.texts.iter().find(|(name, _)| *name == field).map(|(_, text)| text.clone()).expect("a cloud field")
    }

    pub fn local_text(&self, field: &str) -> String {
        self.local_texts.iter().find(|(name, _)| *name == field).map(|(_, text)| text.clone()).expect("a local field")
    }
}

/// Typing: what is in the field is selected and replaced, so the input emits its change as it does for the user.
fn replace_text(input: &Entity<InputState>, text: &str, window: &mut Window, cx: &mut Context<ModelsPage>) {
    input.update(cx, |state, cx| state.replace_all(text.to_owned(), window, cx));
}

fn cloud_name(field: CloudTextField) -> &'static str {
    match field {
        CloudTextField::Name => "name",
        CloudTextField::Address => "address",
        CloudTextField::Model => "model",
        CloudTextField::Key => "key",
        CloudTextField::Temperature => "temperature",
        CloudTextField::TopP => "top_p",
        CloudTextField::MaxTokens => "max_tokens",
        CloudTextField::Effort => "effort",
        CloudTextField::Extra => "extra",
    }
}

fn local_name(field: LocalTextField) -> &'static str {
    match field {
        LocalTextField::Temperature => "temperature",
        LocalTextField::TopP => "top_p",
        LocalTextField::TopK => "top_k",
        LocalTextField::MaxTokens => "max_tokens",
    }
}

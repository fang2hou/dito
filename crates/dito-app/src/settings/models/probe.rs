//! What the screenshot harness steers on the Models page.

use dito_core::secrets::Role;
use gpui_kit::*;

use crate::settings::asr_editor::EditorPreview;
use crate::ui::{self, InputEvent, InputState, TestState};

use super::page::ModelsPage;
use super::polish_field_rules::{CloudPolishField, LocalPolishField};

impl ModelsPage {
    pub(in crate::settings) fn preview(
        &mut self,
        role: Role,
        editor: Option<EditorPreview>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab = role;
        // Each shot starts from the settings alone: nothing typed, scrolled or opened before is left in the page.
        self.cloud_polish = Self::new_cloud_polish_form(window, cx);
        self.cloud_polish_tiles = ui::TileRowState::new();
        self.cloud_polish_menu_open = false;
        self.deleting_cloud_polish = None;
        self.save_failed = false;
        self.cloud_polish_test = TestState::Idle;
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

    /// One step of the polish form probes: typing into a field the way the user does, or pressing one of its
    /// controls. `arg` names the field, the configuration or the model; `text` is what is typed.
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
                let field = CloudPolishField::ALL.into_iter().find(|f| cloud_name(*f) == arg).expect("a field");
                replace_text(&self.cloud_polish[field], text, window, cx);
            }
            "type-local" => {
                let field = LocalPolishField::ALL.into_iter().find(|f| local_name(*f) == arg).expect("a field");
                replace_text(&self.local_polish[field], text, window, cx);
            }
            // A window that is never shown moves no focus, so focus leaving the name is delivered as the input's own
            // event, which is what the page listens to.
            "blur-name" => self.cloud_polish[CloudPolishField::Name].update(cx, |_, cx| cx.emit(InputEvent::Blur)),
            "local-states" => {
                // The first polish model is in use and downloaded, the second is downloaded, the rest are not.
                let specs: Vec<_> = self
                    .st
                    .models
                    .catalog()
                    .iter()
                    .filter(|m| m.kind == dito_local::ModelKind::Polish)
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
                    crate::services::save_config(&self.st, |cfg| cfg.polish.engine.local_model = first);
                }
                cx.notify();
            }
            other => panic!("unknown Models probe step {other:?}"),
        }
    }

    /// What the harness reads of the polish forms after a step.
    pub(in crate::settings) fn probe_state(&self, window: &Window, cx: &App) -> ModelsProbe {
        let cloud = &self.cloud_polish;
        let local = &self.local_polish;
        let name = cloud[CloudPolishField::Name].read(cx);
        ModelsProbe {
            save_failed: self.save_failed,
            menu_open: self.cloud_polish_menu_open,
            name_focused: name.focus_handle(cx).is_focused(window),
            name_selection: name.selected_range(),
            tiles: self.cloud_polish_tiles.probe(),
            test: match &self.cloud_polish_test {
                TestState::Idle => "idle",
                TestState::Running => "running",
                TestState::Ok(_) => "ok",
                TestState::Failed(_) => "failed",
            },
            deleting: self.deleting_cloud_polish.as_ref().map(|deleting| deleting.name.clone()),
            shown: cloud.shown().map(str::to_owned),
            texts: CloudPolishField::ALL.map(|field| (cloud_name(field), cloud.text(field, cx))).into(),
            invalid: CloudPolishField::ALL.into_iter().filter(|f| cloud.invalid(*f, cx)).map(cloud_name).collect(),
            local_shown: local.shown().map(str::to_owned),
            local_texts: LocalPolishField::ALL.map(|field| (local_name(field), local.text(field, cx))).into(),
            local_invalid: LocalPolishField::ALL
                .into_iter()
                .filter(|f| local.invalid(*f, cx))
                .map(local_name)
                .collect(),
            local_models: self
                .st
                .models
                .catalog()
                .iter()
                .filter(|m| m.kind == dito_local::ModelKind::Polish)
                .map(|m| m.id.clone())
                .collect(),
        }
    }
}

/// What the harness reads of the polish forms.
pub struct ModelsProbe {
    pub save_failed: bool,
    /// Whether the menu of the plus button next to the cloud polish tiles is open.
    pub menu_open: bool,
    /// Whether the name field of the cloud polish form has the focus, and which part of its text is selected.
    pub name_focused: bool,
    pub name_selection: std::ops::Range<usize>,
    /// The tile row's scroll offset, the furthest it goes, and the slot a dragged tile is over.
    pub tiles: (f32, f32, Option<usize>),
    /// `idle`, `running`, `ok` or `failed`: the connection test of the cloud polish form.
    pub test: &'static str,
    /// The name the open delete dialog asks about.
    pub deleting: Option<String>,
    /// Id of the cloud polish configuration the form shows, and the text of each of its fields.
    pub shown: Option<String>,
    pub texts: Vec<(&'static str, String)>,
    /// The cloud fields whose text does not parse.
    pub invalid: Vec<&'static str>,
    /// Id of the local polish model the parameters show, their texts and the ones that do not parse.
    pub local_shown: Option<String>,
    pub local_texts: Vec<(&'static str, String)>,
    pub local_invalid: Vec<&'static str>,
    /// Ids of the local polish models.
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

fn cloud_name(field: CloudPolishField) -> &'static str {
    match field {
        CloudPolishField::Name => "name",
        CloudPolishField::Address => "address",
        CloudPolishField::Model => "model",
        CloudPolishField::Key => "key",
        CloudPolishField::Temperature => "temperature",
        CloudPolishField::TopP => "top_p",
        CloudPolishField::MaxTokens => "max_tokens",
        CloudPolishField::Effort => "effort",
        CloudPolishField::Extra => "extra",
    }
}

fn local_name(field: LocalPolishField) -> &'static str {
    match field {
        LocalPolishField::Temperature => "temperature",
        LocalPolishField::TopP => "top_p",
        LocalPolishField::TopK => "top_k",
        LocalPolishField::MaxTokens => "max_tokens",
    }
}

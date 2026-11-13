//! Selection, copy-source, and reset dialogs for per-app prompt rules.

use super::*;
use crate::assets::Ic;
use crate::ui::{BtnKind, cbk, text};

impl PromptsPage {
    pub(super) fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.close_group(Editor::Common, cx);
        self.close_group(Editor::Application, cx);
        self.menu_open = false;
        self.copy_menu_open = false;
        self.dialog_error = None;
        self.picked_app = None;
        self.picked_icon = None;
        self.copy_source = self.target.clone();
        self.dialog = Some(dialog);
        self.dialog_focus.focus(window, cx);
        cx.notify();
    }

    fn choose_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = result.await {
                let app = paths.first().and_then(|p| dito_platform::system::application_at(p));
                let _ = this.update_in(cx, |this, _, cx| {
                    this.dialog_error = if app.is_none() {
                        Some(this.st.cfg.get().general.lang().tr(Tr::PromptsApplicationError).into())
                    } else {
                        None
                    };
                    this.picked_icon =
                        app.as_ref().map(|a| Arc::new(Image::from_bytes(ImageFormat::Png, a.icon_png.clone())));
                    this.picked_app = app;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn add_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app) = self.picked_app.clone() else { return };
        let before = self.drafts.clone();
        if !self.drafts.add_application(&app.bundle_id, &app.name, &self.copy_source) {
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsApplicationDuplicate).into());
            cx.notify();
            return;
        }
        let added_target = self.drafts.for_application(&app.bundle_id);
        self.save(cx);
        if self.save_error.is_some() {
            self.drafts = before;
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsSaveFailed).into());
            return;
        }
        self.target = added_target;
        self.apps = app_choices(&self.drafts);
        self.dialog = None;
        self.load(window, cx);
    }

    /// Shown only for a configured app, never for the Other fallback.
    pub(super) fn remove_app_control(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.target == AppTarget::General {
            return None;
        }
        let p = c.p;
        let id = ElementId::from("remove-prompt-app");
        ui::tip_label(id.clone(), c.t(Tr::PromptsRemoveApplication));
        Some(
            ui::icon_button_with(
                id.clone(),
                &p,
                Ic::Trash,
                false,
                ui::CircleSize::Small,
                cbk(cx, |this: &mut Self, w, cx| this.open_dialog(Dialog::RemoveApp, w, cx)),
            )
            .child(ui::tip_above(&p, &id)),
        )
    }

    fn remove_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let removed_target = self.target.clone();
        let before = self.drafts.clone();
        self.drafts.remove_target(&removed_target);
        self.save(cx);
        if self.save_error.is_some() {
            self.drafts = before;
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsSaveFailed).into());
            return;
        }
        for lang in Lang::ALL {
            self.histories.remove(&(lang, Some(removed_target.clone())));
        }
        self.apps = app_choices(&self.drafts);
        self.target = AppTarget::General;
        self.dialog = None;
        self.load(window, cx);
    }

    fn choice_label(&self, c: &Ctx, choice: &AppChoice) -> SharedString {
        if choice.target == AppTarget::General { c.t(Tr::PromptsOther) } else { choice.name.clone().into() }
    }

    pub(super) fn application_picker(&self, c: &Ctx, cx: &mut Context<Self>, copying: bool) -> Div {
        let p = c.p;
        let id = if copying { "prompt-copy-source" } else { "prompt-application" };
        let selected = if copying { &self.copy_source } else { &self.target };
        let choice = self.apps.iter().find(|a| &a.target == selected).unwrap_or_else(|| self.apps.last().unwrap());
        let open = if copying { self.copy_menu_open } else { self.menu_open };
        let content = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(app_identity(choice, 20., 18., p.text))
            .child(
                text(self.choice_label(c, choice), 14., FontWeight::NORMAL, p.text)
                    .line_height(px(20.))
                    .flex_1()
                    .min_w(px(0.))
                    .truncate(),
            );
        let button = ui::select_trigger(
            id,
            &p,
            open,
            content,
            cbk(cx, move |this: &mut Self, _, cx| {
                if copying {
                    this.copy_menu_open = !this.copy_menu_open;
                } else {
                    this.menu_open = !this.menu_open;
                }
                cx.notify();
            }),
        )
        .w_full();
        let mut control = div().relative().w(px(if copying { 560. } else { 200. })).max_w_full().child(button);
        if open {
            let close = cbk(cx, move |this: &mut Self, _, cx| {
                if copying {
                    this.copy_menu_open = false;
                } else {
                    this.menu_open = false;
                }
                cx.notify();
            });
            let mut panel = ui::menu_panel(ElementId::from((ElementId::from(id), "menu")), &p, close)
                .w(px(if copying { 560. } else { 240. }));
            if !copying {
                panel = panel
                    .child(
                        ui::menu_row(ElementId::from((ElementId::from(id), "add")), &p)
                            .child(
                                div().size(px(24.)).flex_none().flex().items_center().justify_center().child(ui::icon(
                                    Ic::Plus,
                                    20.,
                                    p.text,
                                )),
                            )
                            .child(
                                text(c.t(Tr::PromptsAddApplication), 14., FontWeight::NORMAL, p.text)
                                    .line_height(px(20.)),
                            )
                            .on_click(cx.listener(|this, _, w, cx| this.open_dialog(Dialog::Add, w, cx))),
                    )
                    .child(ui::menu_separator(&p));
            }
            let mut list = div()
                .id((ElementId::from(id), "list"))
                .max_h(px(280.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(2.));
            for (i, app) in self.apps.iter().enumerate() {
                let item_target = app.target.clone();
                let mut row = ui::menu_row(ElementId::from((id, i)), &p);
                row = row.child(app_identity(app, 24., 20., p.text));
                row = row
                    .child(
                        text(self.choice_label(c, app), 14., FontWeight::NORMAL, p.text)
                            .flex_1()
                            .min_w(px(0.))
                            .truncate(),
                    )
                    .child(
                        div()
                            .size(px(14.))
                            .when(&app.target == selected, |d| d.child(ui::icon(Ic::Check, 14., p.accent_text))),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if copying {
                            this.copy_source = item_target.clone();
                            this.copy_menu_open = false;
                            cx.notify();
                        } else {
                            this.switch(this.lang, item_target.clone(), w, cx);
                        }
                    }));
                list = list.child(row);
            }
            panel = panel.child(list);
            control = control.child(ui::float_menu(
                ElementId::from((ElementId::from(id), "menu")),
                ui::CONTROL_H + 6.,
                panel,
            ));
        }
        control
    }

    pub(in crate::settings) fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialog?;
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let close = cbk(cx, |this: &mut Self, w, cx| {
            this.dialog = None;
            this.copy_menu_open = false;
            w.blur(cx);
            cx.notify();
        });
        let title = match dialog {
            Dialog::Reset(_) => Tr::PromptsResetTitle,
            Dialog::RemoveApp => Tr::PromptsRemoveTitle,
            Dialog::Add => Tr::PromptsAddTitle,
        };
        let remove_name = self
            .apps
            .iter()
            .find(|a| a.target == self.target)
            .map(|a| self.choice_label(&c, a).to_string())
            .unwrap_or_default();
        let mut content: Vec<AnyElement> = Vec::new();
        if let Dialog::Reset(editor) = dialog {
            let language = self.lang.name(c.lang);
            let scope = match editor {
                Editor::Common => dito_core::i18n::fmt::reset_common_scope(c.lang, language),
                Editor::Application => dito_core::i18n::fmt::reset_app_scope(c.lang, &remove_name, language),
            };
            content.push(text(scope, 13., FontWeight::NORMAL, p.text).line_height(px(20.)).into_any_element());
        } else if dialog == Dialog::RemoveApp {
            content.push(
                text(
                    dito_core::i18n::fmt::remove_application_scope(c.lang, &remove_name),
                    13.,
                    FontWeight::NORMAL,
                    p.text,
                )
                .line_height(px(20.))
                .into_any_element(),
            );
            content.push(
                text(c.t(Tr::PromptsRemoveHistory), 13., FontWeight::NORMAL, p.muted)
                    .line_height(px(20.))
                    .into_any_element(),
            );
        } else {
            let mut app_slot = div()
                .w_full()
                .h(px(72.))
                .px(px(16.))
                .py(px(16.))
                .rounded(px(12.))
                .border_1()
                .border_color(p.border_strong)
                .bg(p.input_bg)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.));
            if let Some(app) = &self.picked_app {
                let mut identity = div().flex().items_center().gap(px(16.)).flex_1().min_w(px(0.));
                if let Some(icon) = &self.picked_icon {
                    identity = identity.child(img(icon.clone()).size(px(40.)).flex_none());
                }
                app_slot = app_slot
                    .child(identity.child(
                        text(app.name.clone(), 25., FontWeight::MEDIUM, p.text).line_height(px(40.)).truncate(),
                    ));
            } else {
                app_slot = app_slot.child(text(c.t(Tr::PromptsUnselected), 14., FontWeight::NORMAL, p.muted).flex_1());
            }
            app_slot = app_slot.child(ui::button_with(
                "choose-prompt-app",
                &p,
                &c.t(if self.picked_app.is_some() {
                    Tr::PromptsChangeApplication
                } else {
                    Tr::PromptsChooseApplication
                }),
                BtnKind::Primary,
                ui::BtnSize::Large,
                None,
                None,
                cbk(cx, |this: &mut Self, window, cx| this.choose_application(window, cx)),
            ));
            content.push(ui::field(&p, c.t(Tr::PromptsTargetApplication), app_slot).into_any_element());
            content.push(
                ui::field(&p, c.t(Tr::PromptsCopyFrom), self.application_picker(&c, cx, true)).into_any_element(),
            );
        }
        if let Some(error) = &self.dialog_error {
            content.push(text(error.clone(), 13., FontWeight::NORMAL, p.danger).into_any_element());
        }
        let confirm = match dialog {
            Dialog::Reset(editor) => cbk(cx, move |this: &mut Self, w, cx| this.restore(editor, w, cx)),
            Dialog::RemoveApp => cbk(cx, |this: &mut Self, w, cx| this.remove_application(w, cx)),
            Dialog::Add => cbk(cx, |this: &mut Self, w, cx| this.add_application(w, cx)),
        };
        let confirm_label = match dialog {
            Dialog::Reset(_) => Tr::PromptsRestoreDefaults,
            Dialog::RemoveApp => Tr::PromptsRemoveConfirm,
            Dialog::Add => Tr::PromptsAddConfirm,
        };
        Some(ui::confirm_dialog(
            window,
            &p,
            "prompt-dialog",
            &self.dialog_focus,
            c.t(title),
            content,
            ui::DialogAnswer { label: c.t(Tr::AsrEditorCancel), on_click: Some(close) },
            ui::DialogAnswer {
                label: c.t(confirm_label),
                on_click: (dialog != Dialog::Add || self.picked_app.is_some()).then_some(confirm),
            },
            ui::ConfirmStyle::Standard,
        ))
    }
}

fn app_identity(choice: &AppChoice, slot: f32, glyph: f32, color: Hsla) -> Div {
    div().size(px(slot)).flex_none().flex().items_center().justify_center().child(match &choice.icon {
        Some(icon) => img(icon.clone()).size(px(slot)).into_any_element(),
        None => ui::icon(Ic::Apps, glyph, color).into_any_element(),
    })
}

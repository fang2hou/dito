//! Application-profile selection, copying and reset dialogs.

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
        self.target = None;
        self.target_icon = None;
        self.copy_source = self.style.clone();
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
                    this.target_icon =
                        app.as_ref().map(|a| Arc::new(Image::from_bytes(ImageFormat::Png, a.icon_png.clone())));
                    this.target = app;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn add_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app) = self.target.clone() else { return };
        let before = self.drafts.clone();
        if !self.drafts.add_application(&app.bundle_id, &app.name, &self.copy_source) {
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsApplicationDuplicate).into());
            cx.notify();
            return;
        }
        let style = self.drafts.for_application(&app.bundle_id);
        self.save(cx);
        if self.save_error.is_some() {
            self.drafts = before;
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsSaveFailed).into());
            return;
        }
        self.style = style;
        self.apps = app_choices(&self.drafts);
        self.dialog = None;
        self.load(window, cx);
    }

    /// A neutral action appears only for an application profile, never for the Other fallback.
    pub(super) fn remove_app_control(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.style == Style::General {
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
        let style = self.style.clone();
        let before = self.drafts.clone();
        self.drafts.remove_profile(&style);
        self.save(cx);
        if self.save_error.is_some() {
            self.drafts = before;
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsSaveFailed).into());
            return;
        }
        for lang in Lang::ALL {
            self.histories.remove(&(lang, Some(style.clone())));
        }
        self.apps = app_choices(&self.drafts);
        self.style = Style::General;
        self.dialog = None;
        self.load(window, cx);
    }

    fn choice_label(&self, c: &Ctx, choice: &AppChoice) -> SharedString {
        if choice.style == Style::General { c.t(Tr::PromptsOther) } else { choice.name.clone().into() }
    }

    pub(super) fn application_picker(&self, c: &Ctx, cx: &mut Context<Self>, copying: bool) -> Div {
        let p = c.p;
        let id = if copying { "prompt-copy-source" } else { "prompt-application" };
        let selected = if copying { &self.copy_source } else { &self.style };
        let choice = self.apps.iter().find(|a| &a.style == selected).unwrap_or_else(|| self.apps.last().unwrap());
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
                let style = app.style.clone();
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
                            .when(&app.style == selected, |d| d.child(ui::icon(Ic::Check, 14., p.accent_text))),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if copying {
                            this.copy_source = style.clone();
                            this.copy_menu_open = false;
                            cx.notify();
                        } else {
                            this.switch(this.lang, style.clone(), w, cx);
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
            .find(|a| a.style == self.style)
            .map(|a| self.choice_label(&c, a).to_string())
            .unwrap_or_default();
        let mut body = div()
            .w(px(620.))
            .max_w(window.viewport_size().width - px(48.))
            .p(px(28.))
            .rounded(px(20.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.card)
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(text(c.t(title), 20., FontWeight::BOLD, p.text));
        if let Dialog::Reset(editor) = dialog {
            let language = self.lang.name(c.lang);
            let scope = match editor {
                Editor::Common => dito_core::i18n::fmt::reset_common_scope(c.lang, language),
                Editor::Application => dito_core::i18n::fmt::reset_app_scope(c.lang, &remove_name, language),
            };
            body = body.child(text(scope, 13., FontWeight::NORMAL, p.text).line_height(px(20.)));
        } else if dialog == Dialog::RemoveApp {
            body = body
                .child(
                    text(
                        dito_core::i18n::fmt::remove_application_scope(c.lang, &remove_name),
                        13.,
                        FontWeight::NORMAL,
                        p.text,
                    )
                    .line_height(px(20.)),
                )
                .child(text(c.t(Tr::PromptsRemoveHistory), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)));
        } else {
            let mut target = div()
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
            if let Some(app) = &self.target {
                let mut identity = div().flex().items_center().gap(px(16.)).flex_1().min_w(px(0.));
                if let Some(icon) = &self.target_icon {
                    identity = identity.child(img(icon.clone()).size(px(40.)).flex_none());
                }
                target = target
                    .child(identity.child(
                        text(app.name.clone(), 25., FontWeight::MEDIUM, p.text).line_height(px(40.)).truncate(),
                    ));
            } else {
                target = target.child(text(c.t(Tr::PromptsUnselected), 14., FontWeight::NORMAL, p.muted).flex_1());
            }
            target = target.child(ui::button_with(
                "choose-prompt-app",
                &p,
                &c.t(if self.target.is_some() { Tr::PromptsChangeApplication } else { Tr::PromptsChooseApplication }),
                BtnKind::Primary,
                ui::BtnSize::Large,
                None,
                None,
                cbk(cx, |this: &mut Self, window, cx| this.choose_application(window, cx)),
            ));
            body = body.child(ui::field(&p, c.t(Tr::PromptsTargetApplication), target)).child(ui::field(
                &p,
                c.t(Tr::PromptsCopyFrom),
                self.application_picker(&c, cx, true),
            ));
        }
        if let Some(error) = &self.dialog_error {
            body = body.child(text(error.clone(), 13., FontWeight::NORMAL, p.danger));
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
        body = body.child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .gap(px(10.))
                .child(ui::button(
                    "prompt-dialog-cancel",
                    &p,
                    &c.t(Tr::AsrEditorCancel),
                    BtnKind::Secondary,
                    None,
                    close,
                ))
                .child(if dialog == Dialog::Add && self.target.is_none() {
                    ui::disabled_button(
                        "prompt-dialog-confirm",
                        &p,
                        &c.t(confirm_label),
                        BtnKind::Primary,
                        ui::BtnSize::Regular,
                        None,
                        None,
                    )
                    .opacity(0.5)
                } else {
                    ui::button("prompt-dialog-confirm", &p, &c.t(confirm_label), BtnKind::Primary, None, confirm)
                }),
        );
        Some(
            ui::modal_layer("prompt-dialog-overlay", &self.dialog_focus)
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| {
                    if e.keystroke.key == "escape" {
                        this.dialog = None;
                        this.copy_menu_open = false;
                        w.blur(cx);
                        cx.notify();
                        cx.stop_propagation();
                    }
                }))
                .child(body)
                .into_any_element(),
        )
    }
}

fn app_identity(choice: &AppChoice, slot: f32, glyph: f32, color: Hsla) -> Div {
    div().size(px(slot)).flex_none().flex().items_center().justify_center().child(match &choice.icon {
        Some(icon) => img(icon.clone()).size(px(slot)).into_any_element(),
        None => ui::icon(Ic::Apps, glyph, color).into_any_element(),
    })
}

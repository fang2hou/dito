//! Application-profile selection, copying and reset dialogs.

use super::*;
use crate::assets::Ic;
use crate::ui::{BtnKind, cbk, text};

impl PromptsPage {
    pub(super) fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts = self.edited(cx);
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

    fn choose_application(&mut self, cx: &mut Context<Self>) {
        let result =
            cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = result.await {
                let app = paths.first().and_then(|p| dito_platform::system::application_at(p));
                let _ = this.update(cx, |this, cx| {
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
        if !self.drafts.add_application(&app.bundle_id, &app.name, &self.copy_source) {
            self.dialog_error = Some(self.st.cfg.get().general.lang().tr(Tr::PromptsApplicationDuplicate).into());
            cx.notify();
            return;
        }
        self.style = self.drafts.for_application(&app.bundle_id);
        self.apps = app_choices(&self.drafts);
        self.dirty = true;
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
        let mut button =
            ui::field_chrome(id, &p, open).id(id).w_full().px(px(14.)).gap(px(10.)).cursor_pointer().on_click(
                cx.listener(move |this, _, _, cx| {
                    if copying {
                        this.copy_menu_open = !open;
                    } else {
                        this.menu_open = !open;
                    }
                    cx.notify();
                }),
            );
        if let Some(icon) = &choice.icon {
            button = button.child(img(icon.clone()).size(px(20.)).flex_none());
        }
        button = button
            .child(
                text(self.choice_label(c, choice), 14., FontWeight::NORMAL, p.text).flex_1().min_w(px(0.)).truncate(),
            )
            .child(ui::icon(Ic::ChevronDown, 14., p.muted));
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
                            .child(ui::icon(Ic::Plus, 16., p.text))
                            .child(text(c.t(Tr::PromptsAddApplication), 13., FontWeight::NORMAL, p.text))
                            .on_click(cx.listener(|this, _, w, cx| this.open_dialog(Dialog::Add, w, cx))),
                    )
                    .child(div().w_full().h(px(1.)).bg(p.border));
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
                if let Some(icon) = &app.icon {
                    row = row.child(img(icon.clone()).size(px(20.)).flex_none());
                }
                row = row
                    .child(
                        text(self.choice_label(c, app), 13., FontWeight::NORMAL, p.text)
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
            control = control.child(ui::float_menu(ElementId::from((ElementId::from(id), "float")), 44., panel));
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
            .child(text(
                c.t(if dialog == Dialog::Reset { Tr::PromptsResetTitle } else { Tr::PromptsAddTitle }),
                20.,
                FontWeight::BOLD,
                p.text,
            ));
        if dialog == Dialog::Reset {
            body = body
                .child(text(c.t(Tr::PromptsResetScope), 13., FontWeight::NORMAL, p.text).line_height(px(20.)))
                .child(text(c.t(Tr::PromptsResetApps), 13., FontWeight::NORMAL, p.muted).line_height(px(20.)));
        } else {
            let mut target = div()
                .w_full()
                .min_h(px(58.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(12.))
                .border_1()
                .border_color(p.border_strong)
                .bg(p.input_bg)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.));
            if let Some(app) = &self.target {
                let mut identity = div().flex().items_center().gap(px(10.)).flex_1().min_w(px(0.));
                if let Some(icon) = &self.target_icon {
                    identity = identity.child(img(icon.clone()).size(px(20.)));
                }
                target =
                    target.child(identity.child(text(app.name.clone(), 14., FontWeight::MEDIUM, p.text).truncate()));
            }
            target = target.child(ui::button(
                "choose-prompt-app",
                &p,
                &c.t(Tr::PromptsChooseApplication),
                BtnKind::Secondary,
                None,
                cbk(cx, |this: &mut Self, _, cx| this.choose_application(cx)),
            ));
            body = body.child(ui::field(&p, c.t(Tr::PromptsTargetApplication), target)).child(ui::field(
                &p,
                c.t(Tr::PromptsCopyFrom),
                self.application_picker(&c, cx, true),
            ));
            if let Some(error) = &self.dialog_error {
                body = body.child(text(error.clone(), 13., FontWeight::NORMAL, p.danger));
            }
        }
        let confirm = if dialog == Dialog::Reset {
            cbk(cx, |this: &mut Self, w, cx| {
                this.drafts.reset_all();
                services::save_config(&this.st, |c| c.prompts = this.drafts.clone());
                this.apps = app_choices(&this.drafts);
                this.style = Style::General;
                this.dialog = None;
                this.dirty = false;
                this.load(w, cx);
            })
        } else {
            cbk(cx, |this: &mut Self, w, cx| this.add_application(w, cx))
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
                .child(
                    ui::button(
                        "prompt-dialog-confirm",
                        &p,
                        &c.t(if dialog == Dialog::Reset { Tr::PromptsRestoreDefaults } else { Tr::PromptsAddConfirm }),
                        BtnKind::Primary,
                        None,
                        confirm,
                    )
                    .opacity(if dialog == Dialog::Add && self.target.is_none() { 0.5 } else { 1. }),
                ),
        );
        Some(
            div()
                .id("prompt-dialog-overlay")
                .absolute()
                .inset_0()
                .occlude()
                .track_focus(&self.dialog_focus)
                .bg(hsla(0., 0., 0., 0.3))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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

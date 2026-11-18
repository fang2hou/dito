use dito_core::config::{ChatLoading, Source};
use dito_core::i18n::Tr;
use dito_local::catalog::ModelKind;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::services;
use crate::settings::Ctx;
use crate::ui::{self, SelectItem, cbk_idx, text};

const SELECT_ID: &str = "chat-model";
const SELECT_W: f32 = 260.;

/// One choice of the model select: downloaded local text models first, then the text model's cloud configurations.
enum Choice {
    Local(String),
    Cloud(String),
}

impl ModelsPage {
    /// `design:settings/local-models/chat-model`: the card under the text model card on the Text tab.
    pub(super) fn chat_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let same = cfg.chat.same_as_text;
        let header = div()
            .w_full()
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(text(c.t(Tr::ModelsChatModel), 17., FontWeight::BOLD, p.text))
            .child(text(c.t(Tr::ModelsChatModelDesc), 13., FontWeight::NORMAL, p.muted));
        let toggle = cx.entity().downgrade();
        let switch = ui::switch(
            "chat-same-as-text",
            p,
            same,
            std::rc::Rc::new(move |on, _, app| {
                toggle.update(app, |this, cx| this.set_chat_same_as_text(on, cx)).ok();
            }),
        );
        let mut card = ui::card(p).px(px(26.)).pt(px(26.)).pb(px(12.)).gap(px(8.)).child(header).child(chat_row(
            c,
            Tr::ModelsChatSameAsText,
            Tr::ModelsChatSameAsTextDesc,
            switch,
        ));
        if !same {
            card = card.child(chat_row(c, Tr::ModelsModel, Tr::ModelsChatModelPickDesc, self.chat_model_select(c, cx)));
        }
        if cfg.separate_chat_model().is_some() {
            let loading = ui::seg(
                "chat-loading",
                p,
                &[c.t(Tr::ModelsChatLoadingAlways), c.t(Tr::ModelsChatLoadingWhileOpen)],
                usize::from(cfg.chat.loading == ChatLoading::WhileOpen),
                cbk_idx(cx, |this: &mut Self, i, _, cx| this.set_chat_loading(i, cx)),
            );
            card = card.child(chat_row(c, Tr::ModelsChatLoading, Tr::ModelsChatLoadingDesc, loading));
        }
        card
    }

    fn chat_choices(&self) -> Vec<(Choice, SharedString)> {
        let cfg = self.st.cfg.get();
        let models = &self.st.models;
        // The model in use stays listed after its files are deleted, so the select still names it.
        let chosen = (cfg.chat.source == Source::Local).then_some(cfg.chat.local_model.as_str());
        let local = models
            .catalog()
            .iter()
            .filter(|spec| {
                spec.kind == ModelKind::Text && (models.is_installed(&spec.id) || Some(spec.id.as_str()) == chosen)
            })
            .map(|spec| (Choice::Local(spec.id.clone()), SharedString::from(spec.name.clone())));
        let cloud = cfg
            .text_model
            .text_configurations(cfg.general.lang())
            .items
            .iter()
            .map(|item| (Choice::Cloud(item.id.clone()), SharedString::from(item.name.clone())))
            .collect::<Vec<_>>();
        local.chain(cloud).collect()
    }

    fn chat_model_select(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let choices = self.chat_choices();
        let cfg = self.st.cfg.get();
        let engine = cfg.chat_engine();
        let active = engine.active_text().map(|t| t.id.clone());
        let selected = choices
            .iter()
            .position(|(choice, _)| match choice {
                Choice::Local(id) => engine.source == Source::Local && *id == engine.local_model,
                Choice::Cloud(id) => engine.source == Source::Provider && Some(id) == active.as_ref(),
            })
            .unwrap_or(usize::MAX);
        let items: Vec<SelectItem> =
            choices.iter().map(|(_, label)| SelectItem { label: label.clone(), badge: None }).collect();
        ui::select_items(
            SELECT_ID,
            &c.p,
            &items,
            selected,
            self.select == Some(SELECT_ID),
            Some(SELECT_W),
            self.toggle_select(cx, SELECT_ID),
            cbk_idx(cx, |this: &mut Self, i, _, cx| this.pick_chat_model(i, cx)),
        )
    }

    /// Turning it off starts from the text model, so the chat keeps answering with the same model until another is
    /// picked.
    fn set_chat_same_as_text(&mut self, on: bool, cx: &mut Context<Self>) {
        let saved = services::try_save_config(&self.st, |cfg| {
            cfg.chat.same_as_text = on;
            if !on && cfg.chat.local_model.is_empty() && cfg.chat.cloud_text.is_empty() {
                let text = &cfg.text_model;
                cfg.chat.source = text.source;
                cfg.chat.local_model.clone_from(&text.local_model);
                cfg.chat.cloud_text = text.active_text().map(|t| t.id.clone()).unwrap_or_default();
            }
        });
        self.note_save(saved.is_ok());
        cx.notify();
    }

    fn pick_chat_model(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((choice, _)) = self.chat_choices().into_iter().nth(index) else { return };
        let saved = services::try_save_config(&self.st, |cfg| match choice {
            Choice::Local(id) => {
                cfg.chat.source = Source::Local;
                cfg.chat.local_model = id;
            }
            Choice::Cloud(id) => {
                cfg.chat.source = Source::Provider;
                cfg.chat.cloud_text = id;
            }
        });
        self.note_save(saved.is_ok());
        self.select = None;
        cx.notify();
    }

    fn set_chat_loading(&mut self, index: usize, cx: &mut Context<Self>) {
        let loading = if index == 0 { ChatLoading::Always } else { ChatLoading::WhileOpen };
        let saved = services::try_save_config(&self.st, |cfg| cfg.chat.loading = loading);
        self.note_save(saved.is_ok());
        cx.notify();
    }
}

fn chat_row(c: &Ctx, title: Tr, description: Tr, control: impl IntoElement) -> Div {
    let p = &c.p;
    div()
        .w_full()
        .py(px(14.))
        .border_t_1()
        .border_color(p.border)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(text(c.t(title), 14., FontWeight::MEDIUM, p.text))
                .child(text(c.t(description), 12., FontWeight::NORMAL, p.muted)),
        )
        .child(div().flex_none().child(control))
}

use std::rc::Rc;

use dito_core::i18n::Tr;
use dito_local::Storage;
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::page::ModelsPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::ui::{self, BtnKind, cbk, text};

impl ModelsPage {
    pub(super) fn storage_card(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let s = self.st.models.storage();
        let dir = s.dir.clone();
        ui::card(p)
            .px(px(26.))
            .py(px(18.))
            .gap(px(12.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .child(
                        // Let the path column shrink (with an ellipsis) instead of pushing the
                        // buttons out of the card on narrow windows.
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(c.t(Tr::ModelsFolder), 14., FontWeight::BOLD, p.text))
                            .child(ui::muted12(p, s.dir.display().to_string()).truncate()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .child(ui::button(
                                "reveal",
                                p,
                                &c.t(Tr::MenubarShowInFinder),
                                BtnKind::Secondary,
                                Some(Ic::External),
                                Rc::new(move |_, _| {
                                    let _ = std::fs::create_dir_all(&dir);
                                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                                }),
                            ))
                            .child(ui::button(
                                "change-dir",
                                p,
                                &c.t(Tr::ModelsChangeFolder),
                                BtnKind::Secondary,
                                None,
                                cbk(cx, move |this: &mut Self, _, cx| {
                                    this.choose_folder(cx);
                                }),
                            )),
                    ),
            )
            .child(usage_meter(c, &s))
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(dir) = paths.into_iter().next()
                && let Err(e) = st.models.change_dir(&dir.join("Dito Models"))
            {
                tracing::warn!("could not move models: {e}");
            }
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
}

fn usage_meter(c: &Ctx, storage: &Storage) -> Div {
    let p = &c.p;
    let total = (storage.used_bytes + storage.free_bytes).max(1);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .w_full()
                .flex()
                .justify_between()
                .child(text(
                    format!("{} {}", c.t(Tr::ModelsInUse), ui::fmt_bytes(storage.used_bytes)),
                    12.,
                    FontWeight::MEDIUM,
                    p.text,
                ))
                .child(ui::muted12(p, format!("{} {}", c.t(Tr::MenubarFree), ui::fmt_bytes(storage.free_bytes)))),
        )
        .child(ui::progress_bar(p, storage.used_bytes as f32 / total as f32))
}

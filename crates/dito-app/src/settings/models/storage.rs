use std::rc::Rc;

use dito_core::i18n::Tr;
use dito_local::Storage;
use dito_platform::system::spawn_and_reap;
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
        ui::card(p).px(px(26.)).py(px(18.)).gap(px(12.)).child(
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
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(text(c.t(Tr::ModelsFolder), 14., FontWeight::BOLD, p.text))
                                .child(usage_meter(c, &s)),
                        )
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
                                let _ = spawn_and_reap(std::process::Command::new("open").arg(&dir));
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
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let rx =
            cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        let st = self.st.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(dir) = paths.into_iter().next()
            {
                let (models, target) = (st.models.clone(), dir.join("Dito Models"));
                // A move to another volume copies every model, which would freeze the main thread for minutes.
                match st.rt.spawn_blocking(move || models.change_dir(&target)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => tracing::warn!("could not move models: {e}"),
                    Err(e) => tracing::warn!("could not move models: {e}"),
                }
            }
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }
}

fn usage_meter(c: &Ctx, storage: &Storage) -> Div {
    let p = &c.p;
    text(
        format!("{} {}", c.t(Tr::ModelsStorageUsed), ui::fmt_bytes(storage.used_bytes)),
        14.,
        FontWeight::NORMAL,
        p.model_meta,
    )
}

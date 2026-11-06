//! Catalog display metadata shared by every download and engine state.

use dito_core::i18n::{Lang, Tr};
use dito_local::catalog::ModelSpec;
use gpui_kit::SharedString;

use crate::settings::Ctx;
use crate::ui;

pub(super) fn model_metadata(c: &Ctx, spec: &ModelSpec) -> SharedString {
    let size = ui::fmt_bytes(spec.total_size());
    let focus = spec.language_focus.map(|language| {
        c.t(match language {
            Lang::Ja => Tr::ModelsJapaneseFocus,
            Lang::En => Tr::ModelsEnglishFocus,
            Lang::Zh => Tr::ModelsChineseFocus,
        })
    });
    match focus {
        Some(focus) => format!("{size}（{focus}）").into(),
        None => size.into(),
    }
}

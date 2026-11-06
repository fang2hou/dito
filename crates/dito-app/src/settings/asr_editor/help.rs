use dito_core::i18n::{Lang, Tr};
use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::{self, text};

const STRUCTURE: [(&str, Tr); 5] = [
    ("version", Tr::AsrEditorVersion),
    ("connection", Tr::AsrEditorConnection),
    ("language_map", Tr::AsrEditorLanguageMap),
    ("request", Tr::AsrEditorRequest),
    ("response", Tr::AsrEditorResponse),
];

const VARIABLES: [(&str, Tr); 6] = [
    ("{{model}}", Tr::AsrEditorModel),
    ("{{language}}", Tr::AsrEditorMapped),
    ("{{api_key}}", Tr::AsrEditorKey),
    ("{{audio.base64}}", Tr::AsrEditorBase64),
    ("{{audio.data_uri}}", Tr::AsrEditorDataUri),
    ("{{audio.file}}", Tr::AsrEditorWav),
];

const LANGUAGE_MAP_EXAMPLE: &str =
    "{\n  \"ja\": \"JP\", \"en\": \"EN\",\n  \"zh\": \"ZH\", \"auto\": null,\n  \"other\": \"EN\"\n}";

pub(super) fn help_body(p: &Palette, lang: Lang) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(24.))
        .child(div().flex().gap(px(24.)).child(structure_card(p, lang)).child(language_map_card(p, lang)))
        .child(div().flex().gap(px(24.)).child(variables_card(p, lang)).child(request_card(p, lang)))
}

fn card(p: &Palette, lang: Lang, title: Tr, body: Div) -> Div {
    ui::card(p).flex_1().p(px(24.)).gap(px(16.)).child(text(lang.tr(title), 17., FontWeight::BOLD, p.text)).child(body)
}

fn rows(p: &Palette, lang: Lang, items: &[(&str, Tr)]) -> Div {
    let mut rows = div().flex().flex_col().gap(px(10.));
    for (key, label) in items {
        rows = rows.child(
            div()
                .flex()
                .gap(px(16.))
                .child(div().w(px(148.)).child(text(*key, 12., FontWeight::NORMAL, p.muted)))
                .child(text(lang.tr(*label), 13., FontWeight::NORMAL, p.text)),
        );
    }
    rows
}

fn structure_card(p: &Palette, lang: Lang) -> Div {
    card(p, lang, Tr::AsrEditorStructure, rows(p, lang, &STRUCTURE))
}

fn language_map_card(p: &Palette, lang: Lang) -> Div {
    let note = |message: Tr| text(lang.tr(message), 12., FontWeight::NORMAL, p.muted);
    let body = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(text(lang.tr(Tr::AsrEditorInputCodes), 12., FontWeight::MEDIUM, p.text))
        .child(note(Tr::AsrEditorInputAuto))
        .child(note(Tr::AsrEditorExample))
        .child(
            div()
                .p(px(12.))
                .rounded(px(12.))
                .bg(p.sub)
                .child(text(LANGUAGE_MAP_EXAMPLE, 12., FontWeight::NORMAL, p.text).font_family("Menlo")),
        )
        .child(note(Tr::AsrEditorOrder))
        .child(note(Tr::AsrEditorAuto));
    card(p, lang, Tr::AsrEditorLanguageMap, body)
}

fn variables_card(p: &Palette, lang: Lang) -> Div {
    card(p, lang, Tr::AsrEditorVariables, rows(p, lang, &VARIABLES))
}

fn request_card(p: &Palette, lang: Lang) -> Div {
    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(text("request.path", 12., FontWeight::MEDIUM, p.muted))
        .child(text(lang.tr(Tr::AsrEditorPath), 13., FontWeight::NORMAL, p.text))
        .child(text("JSON: body  /  multipart: fields", 12., FontWeight::NORMAL, p.muted))
        .child(text("response.text_pointer", 12., FontWeight::MEDIUM, p.muted))
        .child(text("/choices/0/message/content", 12., FontWeight::NORMAL, p.text))
        .child(text(lang.tr(Tr::AsrEditorPointer), 13., FontWeight::NORMAL, p.muted));
    card(p, lang, Tr::AsrEditorIo, body)
}

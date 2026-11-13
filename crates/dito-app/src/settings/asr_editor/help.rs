//! Cloud speech configuration how-to (`design:settings/cloud-models`): a lead line, four numbered steps in JSON order,
//! then common validation errors.

use dito_core::i18n::{Lang, Tr, fmt};
use gpui_kit::*;

use crate::theme::Palette;
use crate::ui::{self, text};

const CODE_FONT: &str = "Menlo";
const EXAMPLE_W: f32 = 372.;
const API_KEY: &str = "{{api_key}}";

const LANGUAGE_MAP_EXAMPLE: &str = r#""language_map": {
  "ja": "JP",
  "en": "EN",
  "zh": "ZH",
  "auto": null,
  "other": "EN"
}"#;

const PATH_EXAMPLE: &str = r#""request": {
  "method": "POST",
  "path": "/audio/transcriptions",
  "headers": {
    "Authorization": "Bearer {{api_key}}"
  }
}"#;

const BODY_EXAMPLE: &str = r#""encoding": "json",
"body": {
  "model": "{{model}}",
  "input_audio": {
    "data": "{{audio.base64}}",
    "format": "wav"
  },
  "language": "{{language}}"
}"#;

const POINTER_EXAMPLE: &str = r#""response": {
  "text_pointer": "/text"
}"#;

const VARIABLES: [(&str, Tr); 5] = [
    ("{{model}}", Tr::AsrEditorHelpVarModel),
    ("{{language}}", Tr::AsrEditorHelpVarLanguage),
    ("{{audio.base64}}", Tr::AsrEditorHelpVarBase64),
    ("{{audio.data_uri}}", Tr::AsrEditorHelpVarDataUri),
    ("{{audio.file}}", Tr::AsrEditorHelpVarFile),
];

const POINTERS: [(&str, &str); 2] = [
    ("{\"text\": \"\u{2026}\"}", "\u{2192} /text"),
    ("{\"choices\": [{\"message\": {\"content\": \"\u{2026}\"}}]}", "\u{2192} /choices/0/message/content"),
];

struct Step {
    title: Tr,
    field: &'static str,
    body: Tr,
    example: &'static str,
}

pub(super) fn help_body(p: &Palette, lang: Lang) -> Div {
    let note = |message: SharedString| text(message, 13., FontWeight::NORMAL, p.muted).line_height(px(20.));
    let tr = |message: Tr| SharedString::from(lang.tr(message));
    let steps = [
        (
            Step {
                title: Tr::AsrEditorHelpLanguageTitle,
                field: "language_map",
                body: Tr::AsrEditorHelpLanguageBody,
                example: LANGUAGE_MAP_EXAMPLE,
            },
            vec![note(tr(Tr::AsrEditorHelpLanguageAuto)), note(tr(Tr::AsrEditorHelpLanguageOther))],
        ),
        (
            Step {
                title: Tr::AsrEditorHelpPathTitle,
                field: "request.path / request.headers",
                body: Tr::AsrEditorHelpPathBody,
                example: PATH_EXAMPLE,
            },
            vec![note(fmt::asr_help_path_key(lang, API_KEY).into())],
        ),
        (
            Step {
                title: Tr::AsrEditorHelpBodyTitle,
                field: "request.body",
                body: Tr::AsrEditorHelpBodyBody,
                example: BODY_EXAMPLE,
            },
            vec![variables(p, lang), note(tr(Tr::AsrEditorHelpBodyMultipart))],
        ),
        (
            Step {
                title: Tr::AsrEditorHelpPointerTitle,
                field: "response.text_pointer",
                body: Tr::AsrEditorHelpPointerBody,
                example: POINTER_EXAMPLE,
            },
            vec![pointers(p)],
        ),
    ];
    let mut page = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(text(lang.tr(Tr::AsrEditorHelpLead), 14., FontWeight::NORMAL, p.muted).line_height(px(22.)));
    for (index, (step, details)) in steps.into_iter().enumerate() {
        page = page.child(step_card(p, lang, index + 1, &step, details));
    }
    page.child(errors_card(p, lang))
}

fn code(s: &'static str, color: Hsla, line_height: f32) -> Div {
    text(s, 12., FontWeight::NORMAL, color).font_family(CODE_FONT).line_height(px(line_height))
}

fn title(p: &Palette, lang: Lang, title: Tr) -> Div {
    text(lang.tr(title), 17., FontWeight::BOLD, p.text).line_height(px(26.))
}

fn step_card(p: &Palette, lang: Lang, number: usize, step: &Step, details: Vec<Div>) -> Div {
    let badge = div()
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(p.accent)
        .flex()
        .items_center()
        .justify_center()
        .child(text(number.to_string(), 13., FontWeight::SEMIBOLD, p.accent_text).line_height(px(16.)));
    let explanation = div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(badge)
                .child(title(p, lang, step.title).flex_1().min_w(px(0.))),
        )
        .child(code(step.field, p.accent_text, 18.))
        .child(text(lang.tr(step.body), 14., FontWeight::NORMAL, p.text).line_height(px(22.)))
        .children(details);
    let example = div().w(px(EXAMPLE_W)).flex_none().px(px(16.)).py(px(14.)).rounded(px(12.)).bg(p.sub).child(code(
        step.example,
        p.text,
        18.,
    ));
    ui::card(p).p(px(24.)).child(div().flex().items_start().gap(px(24.)).child(explanation).child(example))
}

fn variables(p: &Palette, lang: Lang) -> Div {
    let mut rows = div().py(px(4.)).flex().flex_col().gap(px(6.));
    for (key, meaning) in VARIABLES {
        rows =
            rows.child(div().flex().gap(px(12.)).child(code(key, p.accent_text, 20.).w(px(170.)).flex_none()).child(
                text(lang.tr(meaning), 13., FontWeight::NORMAL, p.text).line_height(px(20.)).flex_1().min_w(px(0.)),
            ));
    }
    rows
}

fn pointers(p: &Palette) -> Div {
    let mut rows = div().py(px(4.)).flex().flex_col().gap(px(8.));
    for (response, pointer) in POINTERS {
        rows = rows
            .child(div().flex().flex_col().child(code(response, p.accent_text, 20.)).child(code(pointer, p.text, 20.)));
    }
    rows
}

fn errors_card(p: &Palette, lang: Lang) -> Div {
    let tr = |message: Tr| SharedString::from(lang.tr(message));
    let errors = [
        (tr(Tr::AsrEditorHelpErrorOther), tr(Tr::AsrEditorHelpFixOther)),
        (fmt::asr_help_error_variable(lang, "{{lang}}").into(), fmt::asr_help_fix_variable(lang, API_KEY).into()),
        (tr(Tr::AsrEditorHelpErrorPath), tr(Tr::AsrEditorHelpFixPath)),
        (tr(Tr::AsrEditorHelpErrorAudioFile), tr(Tr::AsrEditorHelpFixAudioFile)),
    ];
    let mut card = ui::card(p).p(px(24.)).gap(px(10.)).child(title(p, lang, Tr::AsrEditorHelpErrorsTitle));
    for (cause, fix) in errors {
        card = card.child(
            div()
                .flex()
                .gap(px(16.))
                .child(text(cause, 13., FontWeight::MEDIUM, p.text).line_height(px(20.)).w(px(300.)).flex_none())
                .child(text(fix, 13., FontWeight::NORMAL, p.muted).line_height(px(20.)).flex_1().min_w(px(0.))),
        );
    }
    card
}

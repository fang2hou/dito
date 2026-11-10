use std::borrow::Cow;

use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::{ValidationError, ValidationReason, validate};
use serde_json::Value;

/// The 1-based line of the draft a validation failure points at.
pub(super) fn error_row(source: &str) -> usize {
    match serde_json::from_str::<Value>(source) {
        Err(e) => e.line(),
        Ok(value) => {
            let field = validate(&value).err().map(|e| e.field).unwrap_or_default();
            // The key is the last segment of `request.path` or `/connection/model`. A failure without a field, or
            // a key that is not in the draft (a removed `other`), points at the language map.
            let key = field.split(['/', '.']).next_back().filter(|key| !key.is_empty());
            let row_of = |key: &str| {
                let needle = format!("\"{key}\"");
                source.lines().position(|line| line.contains(&needle))
            };
            key.and_then(row_of).or_else(|| row_of("language_map")).unwrap_or(0) + 1
        }
    }
}

/// Parse and validate a draft; the error is the localized message shown under the editor.
pub(super) fn check(source: &str, lang: Lang) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(source).map_err(|e| {
        lang.tr(Tr::FmtAsrSyntaxError)
            .replace("{line}", &e.line().to_string())
            .replace("{column}", &e.column().to_string())
            .replace("{detail}", &e.to_string())
    })?;
    validate(&value).map_err(|e| {
        lang.tr(Tr::FmtAsrFieldError)
            .replace("{line}", &error_row(source).to_string())
            .replace("{detail}", &detail(&e, lang))
    })?;
    Ok(value)
}

/// The failure as the editor words it: the field path, then the reason in the user's language.
fn detail(error: &ValidationError, lang: Lang) -> Cow<'static, str> {
    let reason = reason_text(&error.reason, lang);
    if error.field.is_empty() { reason } else { Cow::Owned(format!("{}: {reason}", error.field)) }
}

fn reason_text(reason: &ValidationReason, lang: Lang) -> Cow<'static, str> {
    let tr = |message| Cow::Borrowed(lang.tr(message));
    match reason {
        ValidationReason::Required => tr(Tr::AsrValidationRequired),
        ValidationReason::UnsupportedVersion => tr(Tr::AsrValidationUnsupportedVersion),
        ValidationReason::InvalidUrl => tr(Tr::AsrValidationInvalidUrl),
        ValidationReason::NotHttpUrl => tr(Tr::AsrValidationNotHttpUrl),
        ValidationReason::ObjectRequired => tr(Tr::AsrValidationObjectRequired),
        ValidationReason::InvalidLanguageMap => tr(Tr::AsrValidationInvalidLanguageMap),
        ValidationReason::InvalidMethod => tr(Tr::AsrValidationInvalidMethod),
        ValidationReason::PathNotRelative => tr(Tr::AsrValidationPathNotRelative),
        ValidationReason::EncodingMismatch => tr(Tr::AsrValidationEncodingMismatch),
        ValidationReason::InvalidJsonPointer => tr(Tr::AsrValidationInvalidJsonPointer),
        ValidationReason::UnknownKey => tr(Tr::AsrValidationUnknownKey),
        ValidationReason::InvalidHeaders => tr(Tr::AsrValidationInvalidHeaders),
        ValidationReason::NonScalarValue => tr(Tr::AsrValidationNonScalarValue),
        ValidationReason::AudioFileOutsideFields => tr(Tr::AsrValidationAudioFileOutsideFields),
        ValidationReason::LanguageInArray => tr(Tr::AsrValidationLanguageInArray),
        ValidationReason::UnclosedVariable => tr(Tr::AsrValidationUnclosedVariable),
        ValidationReason::UnknownVariable { name } => dito_core::i18n::fmt::asr_unknown_variable(lang, name).into(),
        ValidationReason::AudioFileNotWholeValue => tr(Tr::AsrValidationAudioFileNotWholeValue),
        ValidationReason::LanguageNotWholeValue => tr(Tr::AsrValidationLanguageNotWholeValue),
    }
}

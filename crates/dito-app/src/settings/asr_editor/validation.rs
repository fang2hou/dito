use dito_core::i18n::{Lang, Tr};
use dito_core::provider::configured_asr::validate;
use serde_json::Value;

/// The 1-based line of the draft a validation failure points at.
pub(super) fn error_row(source: &str) -> usize {
    match serde_json::from_str::<Value>(source) {
        Err(e) => e.line(),
        Ok(value) => {
            let message = validate(&value).err().map(|e| e.message).unwrap_or_default();
            let path = message.split(':').next().unwrap_or("");
            let key = path.trim_start_matches('/').split(['/', '.']).next_back().unwrap_or(path);
            source
                .lines()
                .position(|l| l.contains(&format!("\"{key}\"")))
                .or_else(|| source.lines().position(|l| l.contains("\"language_map\"")))
                .unwrap_or(0)
                + 1
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
        lang.tr(Tr::FmtAsrFieldError).replace("{line}", &error_row(source).to_string()).replace("{detail}", &e.message)
    })?;
    Ok(value)
}

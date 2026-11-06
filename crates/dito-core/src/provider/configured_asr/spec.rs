use serde_json::Value;

use super::AsrProfiles;
use crate::i18n::Lang;

/// Copyable configuration reference in the current interface language.
/// Only bundled templates are included; user credentials and recordings never enter this text.
pub fn configuration_spec(language: Lang, selected: usize) -> String {
    let guide = match language {
        Lang::Ja => include_str!("../asr-help/ja.md"),
        Lang::En => include_str!("../asr-help/en.md"),
        Lang::Zh => include_str!("../asr-help/zh.md"),
    };
    let profiles = AsrProfiles::default();
    let selected = selected.min(profiles.profiles.len() - 1);
    let template: &Value = &profiles.profiles[selected];
    format!(
        "<dito_speech_model_config_spec>\n{}\n{}\n\n```json\n{}\n```\n</dito_speech_model_config_spec>",
        guide,
        AsrProfiles::NAMES[selected],
        serde_json::to_string_pretty(template).expect("bundled JSON template")
    )
}

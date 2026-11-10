use super::AsrProfiles;
use crate::i18n::Lang;

/// Copyable configuration reference in the current interface language, for the bundled template with `profile_id`
/// (the first one when no bundled template has it).
/// Only bundled templates are included; user credentials and recordings never enter this text.
pub fn configuration_spec(language: Lang, profile_id: &str) -> String {
    let guide = match language {
        Lang::Ja => include_str!("../asr-help/ja.md"),
        Lang::En => include_str!("../asr-help/en.md"),
        Lang::Zh => include_str!("../asr-help/zh.md"),
    };
    let bundled = AsrProfiles::default();
    let template = bundled.profiles.iter().find(|profile| profile.id == profile_id).unwrap_or(&bundled.profiles[0]);
    format!(
        "<dito_speech_model_config_spec>\n{}\n{}\n\n```json\n{}\n```\n</dito_speech_model_config_spec>",
        guide,
        template.name,
        serde_json::to_string_pretty(&template.config).expect("bundled JSON template")
    )
}

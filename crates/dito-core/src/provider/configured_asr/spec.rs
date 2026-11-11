use crate::i18n::Lang;

/// An API key is never part of a request JSON (it lives in the Keychain and the JSON refers to it as `{{api_key}}`),
/// and recordings never enter this text.
pub fn configuration_spec(language: Lang, name: &str, request_json: &str) -> String {
    let guide = match language {
        Lang::Ja => include_str!("../asr-help/ja.md"),
        Lang::En => include_str!("../asr-help/en.md"),
        Lang::Zh => include_str!("../asr-help/zh.md"),
    };
    format!(
        "<dito_speech_model_config_spec>\n{guide}\n{name}\n\n```json\n{request_json}\n```\n</dito_speech_model_config_spec>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CloudAsrTemplate;

    #[test]
    fn the_reference_describes_the_configuration_it_is_given_not_a_bundled_template() {
        let own = "{\n  \"version\": 1,\n  \"connection\": {\n    \"base_url\": \"https://asr.example/v1\",\n    \"model\": \"my-own-model\"\n  }\n}";
        let first_template = serde_json::to_string_pretty(&CloudAsrTemplate::ALL[0].config()).unwrap();
        for (index, language) in Lang::ALL.into_iter().enumerate() {
            let text = configuration_spec(language, "Mine", own);
            assert!(text.starts_with("<dito_speech_model_config_spec>\n"), "language {index}");
            assert!(text.ends_with("\n```\n</dito_speech_model_config_spec>"), "language {index}");
            assert!(text.contains(&format!("\nMine\n\n```json\n{own}\n```\n")), "language {index}: name and own JSON");
            assert!(!text.contains(&first_template), "language {index}: no fallback to the first bundled template");
        }
    }
}

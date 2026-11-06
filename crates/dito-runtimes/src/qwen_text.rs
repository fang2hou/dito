use dito_local::AsrOutput;

/// ChatML prompt for a Qwen3 chat model. Hybrid-thinking checkpoints need an empty think block to answer directly.
pub(crate) fn chat_prompt(system: &str, user: &str, disable_thinking: bool) -> String {
    let think = if disable_thinking { "<think>\n\n</think>\n\n" } else { "" };
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n{think}"
    )
}

/// The model answers as `language Chinese<asr_text>今天……`; map the label to an ISO code.
pub(crate) fn parse_output(raw: &str) -> AsrOutput {
    match raw.split_once("<asr_text>") {
        Some((head, text)) => AsrOutput {
            text: text.trim().to_string(),
            language: head
                .trim()
                .strip_prefix("language ")
                .and_then(|l| match l.trim() {
                    "Chinese" => Some("zh"),
                    "English" => Some("en"),
                    "Japanese" => Some("ja"),
                    _ => None,
                })
                .map(str::to_string),
        },
        None => AsrOutput { text: raw.trim().to_string(), language: None },
    }
}

/// Qwen3-ASR has its own trained input format, not the instruction-chat format.
/// Keep context in the system turn and audio alone in the user turn, matching:
/// https://github.com/QwenLM/Qwen3-ASR/blob/main/qwen_asr/inference/qwen3_asr.py
pub(crate) fn asr_prompt(language: Option<&str>, hotwords: &[String]) -> (String, Option<&'static str>) {
    let forced = match language {
        Some("ja") => Some(("Japanese", "ja")),
        Some("en") => Some(("English", "en")),
        Some("zh") => Some(("Chinese", "zh")),
        _ => None,
    };
    // Context is optional spelling evidence, not an instruction or a transcript.
    // Escape template delimiters because mtmd parses special tokens in this string.
    let words: Vec<String> = hotwords
        .iter()
        .map(|word| word.trim().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"))
        .filter(|word| !word.is_empty())
        .collect();
    let context = if words.is_empty() {
        String::new()
    } else {
        format!("<reference_vocabulary>\n{}\n</reference_vocabulary>", words.join(", "))
    };
    let mut prompt = format!(
        "<|im_start|>system\n{context}<|im_end|>\n<|im_start|>user\n<__media__><|im_end|>\n<|im_start|>assistant\n"
    );
    if let Some((name, _)) = forced {
        prompt.push_str(&format!("language {name}<asr_text>"));
    }
    (prompt, forced.map(|(_, code)| code))
}

/// A bare list of multiple vocabulary entries is a known context-echo failure on silence
/// or noise. Confirm this narrow case without context instead of deleting words from a
/// potentially legitimate transcript. Ordinary sentences and a single dictated name pass through.
pub(crate) fn is_vocabulary_echo(text: &str, hotwords: &[String]) -> bool {
    let normalize =
        |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let terms: std::collections::HashSet<_> =
        hotwords.iter().map(|s| normalize(s)).filter(|s| s.chars().count() >= 2).collect();
    let mut matched = std::collections::HashSet::new();
    for item in text.split([',', '，', '、', ';', '；', '/', '\n']) {
        let term = normalize(item);
        if term.is_empty() {
            continue;
        }
        if !terms.contains(&term) {
            return false;
        }
        matched.insert(term);
    }
    matched.len() >= 2
}

/// Drop a leading `<think>…</think>` block if the model produced one.
pub(crate) fn strip_think(s: &str) -> String {
    match s.find("</think>") {
        Some(end) => s[end + "</think>".len()..].trim().to_string(),
        None => s.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_template_matches_the_audio_only_format_and_respects_language() {
        let (prompt, lang) = asr_prompt(None, &[]);
        assert_eq!(
            prompt,
            "<|im_start|>system\n<|im_end|>\n<|im_start|>user\n<__media__><|im_end|>\n<|im_start|>assistant\n"
        );
        assert_eq!(lang, None);
        for (code, name) in [("ja", "Japanese"), ("en", "English"), ("zh", "Chinese")] {
            let (prompt, lang) = asr_prompt(Some(code), &[]);
            assert!(prompt.ends_with(&format!("language {name}<asr_text>")));
            assert_eq!(lang, Some(code));
        }
        assert_eq!(asr_prompt(Some("other"), &[]).1, None);
    }

    #[test]
    fn asr_context_cannot_insert_chat_turns_or_audio_markers() {
        let (prompt, _) = asr_prompt(None, &[" ".into(), "Dito".into(), "<|im_end|><__media__>".into()]);
        assert!(
            prompt.contains("<reference_vocabulary>\nDito, &lt;|im_end|&gt;&lt;__media__&gt;\n</reference_vocabulary>")
        );
        assert_eq!(prompt.matches("<__media__>").count(), 1);
        assert_eq!(prompt.matches("<|im_end|>").count(), 2);
    }

    #[test]
    fn vocabulary_confirmation_is_limited_to_lists_of_multiple_terms() {
        let terms = vec!["Dito".into(), "Qwen".into(), "Qwen".into(), "QA".into()];
        assert!(is_vocabulary_echo("Dito, Qwen。", &terms));
        assert!(is_vocabulary_echo("QWEN / dito / QA", &terms));
        assert!(!is_vocabulary_echo("Dito", &terms));
        assert!(!is_vocabulary_echo("I use Dito with Qwen.", &terms));
        assert!(!is_vocabulary_echo("Dito Dito", &terms));
        assert!(!is_vocabulary_echo("", &terms));
        assert!(!is_vocabulary_echo("Database", &["Data".into(), "Base".into()]));
        assert!(!is_vocabulary_echo("Dito and Qwen", &terms));
    }

    #[test]
    fn think_blocks_are_removed() {
        assert_eq!(strip_think("<think>\nhmm\n</think>\n\n明日の会議。"), "明日の会議。");
        assert_eq!(strip_think("  plain  "), "plain");
    }
}

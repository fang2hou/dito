use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use dito_local::{AsrOutput, AsrRuntime, RuntimeError};
use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};

use crate::chunking::{join_chunks, split_at_pauses};
use crate::find_file;

/// Languages Dito is used in. SenseVoice also knows others (ko, yue), but on noise it tends to invent a short Korean
/// word; text like that in an unexpected language is dropped.
const EXPECTED: [&str; 4] = ["ja", "en", "zh", "yue"];

/// SenseVoice (zh/en/ja/ko/yue) through sherpa-onnx. The language is fixed when a recognizer is created, so one
/// recognizer is kept per requested language (auto, ja, en, zh), created on first use; the model files are small
/// enough (about 230 MB) that this beats re-loading on every change of the preferred language.
pub struct SenseVoice {
    model: String,
    tokens: String,
    threads: i32,
    recognizers: Mutex<HashMap<&'static str, Recognizer>>,
}

struct Recognizer(SenseVoiceRecognizer);

// SAFETY: the sherpa-onnx recognizer is a C object without thread affinity. It is only moved between threads
// and used through `SenseVoice::recognizers`, whose mutex serializes every call.
unsafe impl Send for Recognizer {}

impl SenseVoice {
    pub fn load(dir: &Path) -> Result<Self, RuntimeError> {
        let model = find_file(dir, |n| n.ends_with(".onnx"))?;
        let tokens = dir.join("tokens.txt");
        if !tokens.exists() {
            return Err(RuntimeError::MissingFile(tokens));
        }
        let threads = std::thread::available_parallelism().map(|n| n.get().min(4) as i32).unwrap_or(2);
        let this = Self {
            model: model.to_string_lossy().into_owned(),
            tokens: tokens.to_string_lossy().into_owned(),
            threads,
            recognizers: Mutex::default(),
        };
        // Loading "auto" now surfaces a broken model at load time instead of on the first dictation.
        let auto = this.create("auto")?;
        this.recognizers.lock().map_err(|_| RuntimeError::LockPoisoned("recognizer"))?.insert("auto", Recognizer(auto));
        Ok(this)
    }

    fn create(&self, language: &str) -> Result<SenseVoiceRecognizer, RuntimeError> {
        SenseVoiceRecognizer::new(SenseVoiceConfig {
            model: self.model.clone(),
            tokens: self.tokens.clone(),
            language: language.into(),
            use_itn: true,
            provider: None,
            num_threads: Some(self.threads),
            debug: false,
        })
        .map_err(RuntimeError::engine)
    }
}

/// The recognizer key for a language hint: only ja, en and zh are forced; anything else is auto-detected.
fn key_for(language: Option<&str>) -> &'static str {
    match language {
        Some("ja") => "ja",
        Some("en") => "en",
        Some("zh") => "zh",
        _ => "auto",
    }
}

fn clean(text: String, lang: Option<String>, forced: bool) -> AsrOutput {
    let unexpected = !forced && lang.as_deref().is_some_and(|l| !EXPECTED.contains(&l));
    let text = if unexpected && text.trim().chars().count() <= 3 { String::new() } else { text };
    AsrOutput { text, language: lang }
}

impl AsrRuntime for SenseVoice {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        _hotwords: &[String],
    ) -> Result<AsrOutput, RuntimeError> {
        let key = key_for(language);
        let mut map = self.recognizers.lock().map_err(|_| RuntimeError::LockPoisoned("recognizer"))?;
        if !map.contains_key(key) {
            map.insert(key, Recognizer(self.create(key)?));
        }
        let Recognizer(r) = map.get_mut(key).expect("just inserted");
        let mut texts: Vec<String> = Vec::new();
        let mut lang = String::new();
        for chunk in split_at_pauses(samples) {
            let out = r.transcribe(16_000, chunk);
            if lang.is_empty() {
                // sherpa reports the language as a tag such as "<|ja|>".
                lang = out.lang.trim_matches(|c| c == '<' || c == '>' || c == '|').to_string();
            }
            let text = out.text;
            if !text.trim().is_empty() {
                texts.push(text);
            }
        }
        Ok(clean(join_chunks(texts), (!lang.is_empty()).then_some(lang), key != "auto"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_map_to_recognizers_and_noise_in_an_unexpected_language_is_dropped() {
        assert_eq!(key_for(Some("ja")), "ja");
        assert_eq!(key_for(Some("ko")), "auto");
        assert_eq!(key_for(None), "auto");
        assert_eq!(clean("그.".into(), Some("ko".into()), false).text, "");
        assert_eq!(clean("안녕하세요 여러분".into(), Some("ko".into()), false).text, "안녕하세요 여러분");
        assert_eq!(clean("はい".into(), Some("ja".into()), false).text, "はい");
        assert_eq!(clean("그.".into(), Some("ko".into()), true).text, "그.", "a forced language is trusted");
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use dito_local::{AsrOutput, AsrRuntime};
use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};

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
    recognizers: Mutex<HashMap<&'static str, SenseVoiceRecognizer>>,
}

// SAFETY: the recognizers wrap C objects that are only ever touched while holding the mutex.
unsafe impl Send for SenseVoice {}
unsafe impl Sync for SenseVoice {}

impl SenseVoice {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let model = find_file(dir, |n| n.ends_with(".onnx"))?;
        let tokens = dir.join("tokens.txt");
        if !tokens.exists() {
            return Err(format!("{} is missing", tokens.display()));
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
        this.recognizers.lock().map_err(|_| "recognizer poisoned".to_string())?.insert("auto", auto);
        Ok(this)
    }

    fn create(&self, language: &str) -> Result<SenseVoiceRecognizer, String> {
        SenseVoiceRecognizer::new(SenseVoiceConfig {
            model: self.model.clone(),
            tokens: self.tokens.clone(),
            language: language.into(),
            use_itn: true,
            provider: None,
            num_threads: Some(self.threads),
            debug: false,
        })
        .map_err(|e| e.to_string())
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
    fn transcribe(&self, samples: &[f32], language: Option<&str>, _hotwords: &[String]) -> Result<AsrOutput, String> {
        let key = key_for(language);
        let mut map = self.recognizers.lock().map_err(|_| "recognizer poisoned".to_string())?;
        if !map.contains_key(key) {
            let r = self.create(key)?;
            map.insert(key, r);
        }
        let r = map.get_mut(key).expect("just inserted");
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

/// SenseVoice-Small was trained on utterances up to 30 s; fed longer clips it loses punctuation and
/// whole syllables. Cut at the quietest 25 ms frame inside the last 6 s of a 28 s stretch so the
/// seam lands in a pause when there is one.
const CHUNK: usize = 28 * 16_000;
const SEARCH_BACK: usize = 6 * 16_000;
const FRAME: usize = 400; // 25 ms

fn split_at_pauses(samples: &[f32]) -> Vec<&[f32]> {
    if samples.len() <= CHUNK {
        return vec![samples];
    }
    let mut cuts = Vec::new();
    let mut start = 0;
    while samples.len() - start > CHUNK {
        let from = start + CHUNK - SEARCH_BACK;
        let mut best = from;
        let mut best_energy = f32::INFINITY;
        let mut i = (from / FRAME + 1) * FRAME; // first frame fully inside the search span
        while i + FRAME <= start + CHUNK {
            let energy = samples[i..i + FRAME].iter().map(|s| s * s).sum::<f32>();
            if energy < best_energy {
                best_energy = energy;
                best = i + FRAME / 2;
            }
            i += FRAME;
        }
        cuts.push(best);
        start = best;
    }
    cuts.dedup();
    let mut spans: Vec<&[f32]> = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0;
    for cut in cuts {
        spans.push(&samples[start..cut]);
        start = cut;
    }
    spans.push(&samples[start..]);
    spans
}

/// CJK chunk seams carry their own punctuation and take no separator; Latin seams get a space so
/// words from neighboring chunks cannot merge.
fn join_chunks(texts: Vec<String>) -> String {
    let mut out = String::new();
    for text in texts {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty()
            && out.ends_with(|c: char| c.is_ascii_alphanumeric())
            && text.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            out.push(' ');
        }
        out.push_str(text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 s of tone then 1 s of silence, repeated — pauses land every 4 s.
    fn pattern(seconds: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(seconds * 16_000);
        while v.len() < seconds * 16_000 {
            v.extend(std::iter::repeat_n(0.3, 3 * 16_000));
            v.extend(std::iter::repeat_n(0.0, 16_000));
        }
        v.truncate(seconds * 16_000);
        v
    }

    #[::core::prelude::v1::test]
    fn short_clips_stay_whole() {
        let v = pattern(20);
        let spans = split_at_pauses(&v);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].len(), v.len());
    }

    #[::core::prelude::v1::test]
    fn long_clips_are_split_inside_pauses() {
        let v = pattern(60);
        let spans = split_at_pauses(&v);
        assert!(spans.len() > 1, "60 s must be split, got {} spans", spans.len());
        assert!(spans.iter().all(|s| s.len() <= CHUNK), "a span exceeds the model's window");
        for span in &spans {
            let seam = span.last().expect("nonempty span");
            assert!(seam.abs() < 0.05, "cut landed in speech, amplitude {}", seam);
        }
        assert_eq!(spans.iter().map(|s| s.len()).sum::<usize>(), v.len(), "samples lost at a seam");
    }

    #[::core::prelude::v1::test]
    fn continuous_speech_is_split_anyway() {
        let v = vec![0.3; CHUNK + 16_000];
        let spans = split_at_pauses(&v);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans.iter().map(|s| s.len()).sum::<usize>(), v.len());
    }

    #[::core::prelude::v1::test]
    fn hints_map_to_recognizers_and_noise_in_an_unexpected_language_is_dropped() {
        assert_eq!(key_for(Some("ja")), "ja");
        assert_eq!(key_for(Some("ko")), "auto");
        assert_eq!(key_for(None), "auto");
        assert_eq!(clean("그.".into(), Some("ko".into()), false).text, "");
        assert_eq!(clean("안녕하세요 여러분".into(), Some("ko".into()), false).text, "안녕하세요 여러분");
        assert_eq!(clean("はい".into(), Some("ja".into()), false).text, "はい");
        assert_eq!(clean("그.".into(), Some("ko".into()), true).text, "그.", "a forced language is trusted");
    }

    #[::core::prelude::v1::test]
    fn chunk_seams_join_without_merging_words() {
        assert_eq!(join_chunks(vec!["第一段。".into(), "第二段。".into()]), "第一段。第二段。");
        assert_eq!(join_chunks(vec!["first part".into(), "second part".into()]), "first part second part");
        assert_eq!(join_chunks(vec!["value is 300".into(), "毫秒左右".into()]), "value is 300毫秒左右");
        assert_eq!(join_chunks(vec!["".into(), "  ".into()]), "");
    }
}

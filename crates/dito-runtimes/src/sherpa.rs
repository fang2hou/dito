use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use dito_local::{AsrOutput, AsrRuntime, RuntimeError};
use std::ffi::{CStr, CString};
use std::ptr::NonNull;

use sherpa_onnx_sys::offline_asr as ffi;

use crate::chunking::{join_chunks, split_at_pauses};

/// SenseVoice also knows ko, but on noise it tends to invent a short Korean word; text in an unexpected language is
/// dropped.
const EXPECTED: [&str; 4] = ["ja", "en", "zh", "yue"];

/// The language is fixed when a recognizer is created, so one recognizer is kept per requested language, created on
/// first use. The model files are small enough (about 230 MB) that this beats re-loading on every change of the
/// preferred language.
pub struct SenseVoice {
    model: String,
    tokens: String,
    recognizers: Mutex<HashMap<&'static str, Recognizer>>,
}

/// All calls are serialized by the adapter's mutex.
struct Recognizer(NonNull<ffi::OfflineRecognizer>);

// SAFETY: the native object has no thread affinity. Its owner transfers it between
// threads, and all inference calls require exclusive access under a mutex.
unsafe impl Send for Recognizer {}

impl Recognizer {
    fn create(config: &ffi::OfflineRecognizerConfig) -> Result<Self, RuntimeError> {
        // SAFETY: config strings outlive this call. The C API copies the config and
        // returns an owned recognizer, or null when validation fails.
        let ptr = unsafe { ffi::SherpaOnnxCreateOfflineRecognizer(config) };
        NonNull::new(ptr.cast_mut())
            .map(Self)
            .ok_or_else(|| RuntimeError::engine("could not create sherpa-onnx recognizer"))
    }

    /// A native decode call cannot be interrupted, so `stop` is checked before each call: once for a short clip,
    /// and at every seam between the chunks of a long one.
    fn transcribe(&mut self, samples: &[f32], stop: &AtomicBool) -> Result<AsrOutput, RuntimeError> {
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        let length = i32::try_from(samples.len()).map_err(RuntimeError::engine)?;
        // SAFETY: self is a live recognizer, exclusively borrowed for the entire call.
        let ptr = unsafe { ffi::SherpaOnnxCreateOfflineStream(self.0.as_ptr()) };
        let stream = Stream(
            NonNull::new(ptr.cast_mut()).ok_or_else(|| RuntimeError::engine("could not create sherpa-onnx stream"))?,
        );
        // SAFETY: samples remain valid during AcceptWaveform, which copies them.
        // The stream and recognizer remain alive until decoding completes.
        let json = unsafe {
            ffi::SherpaOnnxAcceptWaveformOffline(stream.0.as_ptr(), 16_000, samples.as_ptr(), length);
            ffi::SherpaOnnxDecodeOfflineStream(self.0.as_ptr(), stream.0.as_ptr());
            let result = ffi::SherpaOnnxGetOfflineStreamResultAsJson(stream.0.as_ptr());
            if result.is_null() {
                return Err(RuntimeError::engine("sherpa-onnx returned no recognition result"));
            }
            let json = CStr::from_ptr(result).to_string_lossy().into_owned();
            ffi::SherpaOnnxDestroyOfflineStreamResultJson(result);
            json
        };
        // Preserve SenseVoice's detected-language field. The high-level official
        // wrapper currently omits it from its result type, so use the official C ABI.
        let result: serde_json::Value = serde_json::from_str(&json).map_err(RuntimeError::engine)?;
        let text = result
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RuntimeError::engine("sherpa-onnx result has no text"))?;
        let language = result
            .get("lang")
            .and_then(|v| v.as_str())
            .map(|s| s.trim_matches(|c| c == '<' || c == '>' || c == '|'))
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        Ok(AsrOutput { text: text.into(), language })
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner, and all streams have already been dropped.
        unsafe {
            ffi::SherpaOnnxDestroyOfflineRecognizer(self.0.as_ptr());
        }
    }
}

struct Stream(NonNull<ffi::OfflineStream>);
impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: this owns the stream returned by CreateOfflineStream.
        unsafe {
            ffi::SherpaOnnxDestroyOfflineStream(self.0.as_ptr());
        }
    }
}

fn config(tokens: &CStr, feature_dim: i32) -> ffi::OfflineRecognizerConfig {
    // SAFETY: the C API explicitly accepts zero-initialized configs. These structs
    // contain only C pointers and numeric fields; the required fields are set below.
    let mut config: ffi::OfflineRecognizerConfig = unsafe { std::mem::zeroed() };
    config.feat_config.sample_rate = 16_000;
    config.feat_config.feature_dim = feature_dim;
    config.model_config.tokens = tokens.as_ptr();
    config.model_config.num_threads = std::thread::available_parallelism().map(|n| n.get().min(4) as i32).unwrap_or(2);
    config.model_config.provider = c"cpu".as_ptr();
    config.model_config.debug = i32::from(std::env::var_os("DITO_SHERPA_DEBUG").is_some());
    config.decoding_method = c"greedy_search".as_ptr();
    config
}

fn cstring(value: &str) -> Result<CString, RuntimeError> {
    CString::new(value).map_err(RuntimeError::engine)
}

impl SenseVoice {
    pub fn load(model: &Path, tokens: &Path) -> Result<Self, RuntimeError> {
        let this = Self {
            model: model.to_string_lossy().into_owned(),
            tokens: tokens.to_string_lossy().into_owned(),
            recognizers: Mutex::default(),
        };
        // Loading "auto" surfaces a broken model at load time instead of on the first dictation.
        let auto = this.create("auto")?;
        this.recognizers.lock().map_err(|_| RuntimeError::LockPoisoned("recognizer"))?.insert("auto", auto);
        Ok(this)
    }

    fn create(&self, language: &str) -> Result<Recognizer, RuntimeError> {
        let (model, tokens, language) = (cstring(&self.model)?, cstring(&self.tokens)?, cstring(language)?);
        let mut config = config(&tokens, 80);
        config.model_config.sense_voice.model = model.as_ptr();
        config.model_config.sense_voice.language = language.as_ptr();
        config.model_config.sense_voice.use_itn = 1;
        Recognizer::create(&config)
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
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        let key = key_for(language);
        let mut map = self.recognizers.lock().map_err(|_| RuntimeError::LockPoisoned("recognizer"))?;
        // A call cancelled while it waited for the lock must not load a recognizer.
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        if !map.contains_key(key) {
            map.insert(key, self.create(key)?);
        }
        let r = map.get_mut(key).expect("just inserted");
        let mut texts: Vec<String> = Vec::new();
        let mut lang = String::new();
        for chunk in split_at_pauses(samples) {
            let out = r.transcribe(chunk, stop)?;
            if lang.is_empty() {
                // sherpa reports the language as a tag such as "<|ja|>".
                lang = out.language.unwrap_or_default();
            }
            let text = out.text;
            if !text.trim().is_empty() {
                texts.push(text);
            }
        }
        Ok(clean(join_chunks(texts), (!lang.is_empty()).then_some(lang), key != "auto"))
    }
}

/// Haas packs its KDA state into the standard transducer state interface; no model-specific runtime is needed.
pub struct NemoTdt {
    recognizer: Mutex<Recognizer>,
}

impl NemoTdt {
    pub fn load(
        encoder: &Path,
        decoder: &Path,
        joiner: &Path,
        tokens: &Path,
        feature_dim: i32,
    ) -> Result<Self, RuntimeError> {
        let (encoder, decoder, joiner, tokens) = (
            cstring(&encoder.to_string_lossy())?,
            cstring(&decoder.to_string_lossy())?,
            cstring(&joiner.to_string_lossy())?,
            cstring(&tokens.to_string_lossy())?,
        );
        let mut config = config(&tokens, feature_dim);
        config.model_config.transducer.encoder = encoder.as_ptr();
        config.model_config.transducer.decoder = decoder.as_ptr();
        config.model_config.transducer.joiner = joiner.as_ptr();
        config.model_config.model_type = c"nemo_transducer".as_ptr();
        config.model_config.modeling_unit = c"cjkchar".as_ptr();
        let recognizer = Recognizer::create(&config)?;
        Ok(Self { recognizer: Mutex::new(recognizer) })
    }
}

impl AsrRuntime for NemoTdt {
    fn transcribe(
        &self,
        samples: &[f32],
        _language: Option<&str>,
        _hotwords: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        // Language hints are not supported by this layout. Do not block other languages
        // or label every output Japanese: this decoder does not detect language.
        if samples.iter().all(|s| s.abs() <= f32::EPSILON) {
            return Ok(AsrOutput { text: String::new(), language: None });
        }
        let mut recognizer = self.recognizer.lock().map_err(|_| RuntimeError::LockPoisoned("recognizer"))?;
        let texts = split_at_pauses(samples)
            .into_iter()
            .map(|chunk| recognizer.transcribe(chunk, stop).map(|out| out.text))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AsrOutput { text: join_chunks(texts), language: None })
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

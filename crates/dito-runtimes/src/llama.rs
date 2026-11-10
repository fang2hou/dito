use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use dito_local::{AsrOutput, AsrRuntime, LlmRuntime, RuntimeError};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::mtmd::{MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use llama_cpp_2::sampling::LlamaSampler;

use crate::qwen_text::{asr_prompt, chat_prompt, is_vocabulary_echo, parse_output, strip_think};

fn backend() -> Result<&'static LlamaBackend, RuntimeError> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|message| RuntimeError::Engine(message.clone()))
}

/// A Qwen3 chat model (ChatML) served by llama.cpp with Metal offload.
pub struct Chat {
    model: LlamaModel,
    gate: Mutex<()>,
    /// Hybrid-thinking checkpoints need an empty think block to answer directly.
    disable_thinking: bool,
}

impl Chat {
    pub fn load(path: &Path, disable_thinking: bool) -> Result<Self, RuntimeError> {
        let params = LlamaModelParams::default().with_n_gpu_layers(1000);
        let model = LlamaModel::load_from_file(backend()?, path, &params).map_err(RuntimeError::engine)?;
        Ok(Self { model, gate: Mutex::new(()), disable_thinking })
    }
}

impl LlmRuntime for Chat {
    fn generate(&self, system: &str, user: &str, stop: &AtomicBool) -> Result<String, RuntimeError> {
        let _one_at_a_time = self.gate.lock().map_err(|_| RuntimeError::LockPoisoned("model lock"))?;
        // A call cancelled while it waited for the gate must not start decoding.
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        let backend = backend()?;
        // Special tokens (<|im_start|> ...) must be parsed; Qwen adds no BOS.
        let tokens =
            self.model.vocab().tokenize(chat_prompt(system, user, self.disable_thinking).as_bytes(), false, true);
        // A polish rewrite runs about as long as the input; long dictations need the headroom.
        let budget = (tokens.len() as u32 * 3 / 2).clamp(256, 4096);
        let n_ctx = (tokens.len() as u32 + budget).max(1024);
        let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4);
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut ctx = self.model.new_context(backend, params).map_err(RuntimeError::engine)?;

        let mut batch = LlamaBatch::new(tokens.len().max(512), 1);
        let last = tokens.len() as i32 - 1;
        for (i, t) in (0_i32..).zip(tokens.iter().copied()) {
            batch.add(t, i, &[0], i == last).map_err(RuntimeError::engine)?;
        }
        ctx.decode(&mut batch).map_err(RuntimeError::engine)?;

        let mut sampler = LlamaSampler::chain_simple([LlamaSampler::temp(0.2), LlamaSampler::dist(1234)]);
        let prompt_len = batch.n_tokens();
        let generated =
            generate_text(&self.model, &mut ctx, &mut sampler, &mut batch, prompt_len, budget as usize, stop)?;
        // Cut-off text must not pass for a finished rewrite.
        if generated.truncated {
            return Err(RuntimeError::Truncated);
        }
        Ok(strip_think(&generated.text))
    }
}

struct Generated {
    text: String,
    /// `max_tokens` ran out before the model emitted an end-of-generation token.
    truncated: bool,
}

/// Samples and decodes one token at a time until the model ends its turn or `max_tokens` are produced.
/// `stop` is checked before every token, so a cancel takes effect within one token's decode time.
/// `batch` holds the tokens decoded last, and the next token is sampled from the logits of its final entry
/// (an empty batch means the context's most recent output).
fn generate_text(
    model: &LlamaModel,
    ctx: &mut LlamaContext,
    sampler: &mut LlamaSampler,
    batch: &mut LlamaBatch,
    first_position: i32,
    max_tokens: usize,
    stop: &AtomicBool,
) -> Result<Generated, RuntimeError> {
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut out = String::new();
    for position in (first_position..).take(max_tokens) {
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.vocab().is_eog(token) {
            return Ok(Generated { text: out, truncated: false });
        }
        let bytes = model.vocab().token_to_piece(token, true, None);
        // encoding_rs never grows the destination: reserve the worst case for this piece.
        let mut piece =
            String::with_capacity(decoder.max_utf8_buffer_length(bytes.len()).unwrap_or(bytes.len() * 3 + 4));
        let _ = decoder.decode_to_string(&bytes, &mut piece, false);
        out.push_str(&piece);
        batch.clear();
        batch.add(token, position, &[0], true).map_err(RuntimeError::engine)?;
        ctx.decode(batch).map_err(RuntimeError::engine)?;
    }
    Ok(Generated { text: out, truncated: true })
}

/// Qwen3-ASR (LLM GGUF + mmproj audio tower) through llama.cpp mtmd. The model and audio tower
/// stay resident; each clip gets a fresh context because audio tokens fill the KV cache. The
/// mutex doubles as the one-at-a-time gate and the `&mut` mtmd needs to tokenize.
pub struct Asr {
    mtmd: Mutex<MtmdContext>,
    model: LlamaModel,
    threads: i32,
}

/// Matches the recorder's memory cap (10 min, `dito-audio` `MAX_SAMPLES`). The upstream long-audio
/// defect this once guarded against (llama.cpp #21847) was fixed on 2026-05-15 (commit 72e60f5) and
/// the vendored llama.cpp already contains it — a 72.7 s real-world clip transcribes in full.
const MAX_SAMPLES: usize = 600 * 16_000;

impl Asr {
    pub fn load(model_path: &Path, mmproj_path: &Path) -> Result<Self, RuntimeError> {
        let model =
            LlamaModel::load_from_file(backend()?, model_path, &LlamaModelParams::default().with_n_gpu_layers(1000))
                .map_err(RuntimeError::engine)?;
        let mmproj = mmproj_path.to_str().ok_or_else(|| RuntimeError::engine("mmproj path is not utf-8"))?;
        let mtmd =
            MtmdContext::init_from_file(mmproj, &model, &MtmdContextParams::default()).map_err(RuntimeError::engine)?;
        if !mtmd.support_audio() {
            return Err(RuntimeError::AudioUnsupported);
        }
        let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4);
        Ok(Self { mtmd: Mutex::new(mtmd), model, threads })
    }
}

impl AsrRuntime for Asr {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        hotwords: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        let (prompt, forced_language) = asr_prompt(language, hotwords);
        // Do not run autoregressive decoding on digital silence: it can invent a fluent answer.
        // The pipeline additionally applies its existing quiet/short-clip gate before calling us.
        if samples.iter().all(|s| s.abs() <= f32::EPSILON) {
            return Ok(AsrOutput { text: String::new(), language: forced_language.map(str::to_string) });
        }
        let output = self.transcribe_prompt(samples, prompt, forced_language, stop)?;
        if is_vocabulary_echo(&output.text, hotwords) {
            tracing::debug!("ASR returned a vocabulary list; confirming without context");
            let (prompt, forced_language) = asr_prompt(language, &[]);
            return self.transcribe_prompt(samples, prompt, forced_language, stop);
        }
        Ok(output)
    }
}

impl Asr {
    fn transcribe_prompt(
        &self,
        samples: &[f32],
        prompt: String,
        forced_language: Option<&str>,
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        let mut mtmd = self.mtmd.lock().map_err(|_| RuntimeError::LockPoisoned("model lock"))?;
        // A call cancelled while it waited for the lock must not start encoding audio.
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        let samples = &samples[..samples.len().min(MAX_SAMPLES)];
        let bitmap = MtmdBitmap::from_audio_data(samples).map_err(RuntimeError::engine)?;

        let secs = (samples.len() / 16_000) as u32;
        // Spoken zh/ja costs roughly one token per character; 8 tokens/s covers fast speech with margin.
        let max_out = (secs * 8).clamp(512, 4096);
        // Audio costs ~13.5 tokens per second (measured); prompt and answer need the rest.
        let prompt_tokens = self.model.vocab().tokenize(prompt.as_bytes(), false, true).len() as u32;
        let n_ctx = (secs * 14 + max_out + prompt_tokens + 128).max(1024);
        let mut ctx = self
            .model
            .new_context(
                backend()?,
                LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(n_ctx))
                    .with_n_threads(self.threads)
                    .with_n_threads_batch(self.threads),
            )
            .map_err(RuntimeError::engine)?;

        let text = MtmdInputText { text: prompt, add_special: false, parse_special: true };
        let chunks = mtmd.tokenize(text, &[&bitmap]).map_err(RuntimeError::engine)?;
        let n_past = chunks.eval_chunks(&mut mtmd, &mut ctx, 0, 0, 512, true).map_err(RuntimeError::engine)?;

        // Greedy decode: dictation wants the literal best guess, not variety.
        let mut sampler = LlamaSampler::chain_simple([LlamaSampler::temp(0.0), LlamaSampler::dist(0)]);
        let mut batch = LlamaBatch::new(1, 1);
        // A transcript that used its whole budget is still returned: partial text beats none.
        let generated = generate_text(&self.model, &mut ctx, &mut sampler, &mut batch, n_past, max_out as usize, stop)?;
        let mut result = parse_output(&generated.text);
        if let Some(language) = forced_language {
            result.language = Some(language.into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires DITO_POLISH_MODEL_DIR and a local Qwen3.5-4B GGUF"]
    fn polish_reference_vocabulary_regression() {
        use dito_core::prompts::{PromptInput, Prompts, Style, build_system_prompt};
        use dito_local::LlmRuntime;
        use std::sync::atomic::AtomicBool;
        let spec = dito_local::catalog::default_catalog().into_iter().find(|s| s.id == "qwen3.5-4b-instruct").unwrap();
        let dito_local::catalog::ModelFormat::Chat { model, disable_thinking } = spec.format else { unreachable!() };
        let path = std::path::PathBuf::from(std::env::var("DITO_POLISH_MODEL_DIR").unwrap()).join(model);
        let model = super::Chat::load(&path, disable_thinking).unwrap();
        let terms = vec!["NebulaBanana".into(), "银河量子仓库".into()];
        let prompts = Prompts::default();
        let stop = AtomicBool::new(false);
        for text in
            ["嗯，明天下午三点开会。", "えーと、明日の午後三時に会議をします。", "Um, let's meet tomorrow at three."]
        {
            let system = build_system_prompt(&PromptInput {
                prompts: &prompts,
                style: Style::General,
                text,
                dictionary: &terms,
            });
            let out = model.generate(&system, text, &stop).unwrap();
            eprintln!("POLISH_REGRESSION {text}: {out}");
            assert!(!out.is_empty());
            for term in &terms {
                assert!(!out.contains(term));
            }
            assert!(!out.contains("reference_vocabulary"));
        }
    }

    /// Run explicitly against local GGUF files; no downloads or paid provider calls.
    #[test]
    #[ignore = "requires DITO_ASR_MODEL_DIR and the local bench audio clips (`mise run bench clips`)"]
    fn asr_reference_vocabulary_audio_regression() {
        use dito_local::AsrRuntime;
        use std::sync::atomic::AtomicBool;
        let directory = std::path::PathBuf::from(std::env::var("DITO_ASR_MODEL_DIR").unwrap());
        let spec = dito_local::catalog::default_catalog()
            .into_iter()
            .filter(|s| s.kind == dito_local::catalog::ModelKind::Asr)
            .find(|s| s.format.files().iter().all(|name| directory.join(name).is_file()))
            .expect("catalog ASR files");
        let dito_local::catalog::ModelFormat::QwenAsr { model, projector } = spec.format else { unreachable!() };
        let model = super::Asr::load(&directory.join(model), &directory.join(projector)).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/audio");
        let unrelated = vec!["星际菠萝研究所".into(), "银河量子仓库".into(), "NebulaBanana".into()];
        let stop = AtomicBool::new(false);
        for (file, lang, relevant) in
            [("zh_01.wav", "zh", "会议室"), ("ja_01.wav", "ja", "新機能"), ("en_01.wav", "en", "QA")]
        {
            let clip = dito_core::audio::AudioClip::from_wav(&std::fs::read(root.join(file)).unwrap()).unwrap();
            for (name, words) in
                [("none", vec![]), ("relevant", vec![relevant.into()]), ("unrelated", unrelated.clone())]
            {
                let result = model.transcribe(&clip.samples, Some(lang), &words, &stop).unwrap();
                eprintln!("ASR_REGRESSION {file} {name}: {}", result.text);
                assert!(!result.text.trim().is_empty());
                assert_eq!(result.language.as_deref(), Some(lang));
                for word in &unrelated {
                    assert!(!result.text.contains(word), "unspoken term leaked into {file}: {}", result.text);
                }
                assert!(!result.text.contains("reference_vocabulary"));
            }
        }
        let silence = dito_core::audio::AudioClip::silence(2.);
        let result = model.transcribe(&silence.samples, Some("zh"), &unrelated, &stop).unwrap();
        eprintln!("ASR_REGRESSION silence: {}", result.text);
        assert!(result.text.trim().is_empty(), "silence must not become reference words");
        // Deterministic non-speech above the pipeline's energy gate. Tags alone made this
        // model recite all three unrelated entries; the context-free confirmation must not.
        let noise: Vec<f32> = (0..32000)
            .map(|i| (((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8) as f32 / 16777216. - 0.5) * 0.03)
            .collect();
        let result = model.transcribe(&noise, Some("zh"), &unrelated, &stop).unwrap();
        eprintln!("ASR_REGRESSION noise: {}", result.text);
        for word in &unrelated {
            assert!(!result.text.contains(word));
        }
    }
}

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use dito_local::{AsrOutput, AsrRuntime, LlmRuntime, RuntimeError, Sampling};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaChatMessage;
use llama_cpp_2::model::LlamaChatTemplate;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::mtmd::{MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::qwen_text::{asr_prompt, is_vocabulary_echo, parse_output, strip_think};

fn backend() -> Result<&'static LlamaBackend, RuntimeError> {
    static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|message| RuntimeError::Engine(message.clone()))
}

/// Qwen hybrid-thinking checkpoints answer directly when the assistant turn opens with an empty think block.
const EMPTY_THINK_BLOCK: &str = "<think>\n\n</think>\n\n";

/// The seed of a polish reply: the same input and sampling give the same text.
const POLISH_SEED: u32 = 1234;

/// How many tokens a polish reply may take. A rewrite runs about as long as the input and long dictations need the
/// headroom; `cap` (the user's `max_tokens` or the catalog's) can only lower that.
fn output_budget(prompt_tokens: usize, cap: Option<u32>) -> u32 {
    let computed = (prompt_tokens as u32 * 3 / 2).clamp(256, 4096);
    cap.map_or(computed, |cap| computed.min(cap))
}

/// The cuts first, then the temperature, as llama.cpp orders its own default chain. Without a top-k or top-p the chain is
/// the temperature alone.
fn polish_sampler(sampling: &Sampling) -> LlamaSampler {
    let mut chain = Vec::with_capacity(4);
    if let Some(k) = sampling.top_k {
        chain.push(LlamaSampler::top_k(i32::try_from(k).unwrap_or(i32::MAX)));
    }
    if let Some(p) = sampling.top_p {
        chain.push(LlamaSampler::top_p(p, 1));
    }
    chain.push(LlamaSampler::temp(sampling.temperature));
    chain.push(LlamaSampler::dist(POLISH_SEED));
    LlamaSampler::chain_simple(chain)
}

/// A chat model served by llama.cpp with Metal offload. Its prompt format is the chat template embedded in the GGUF.
pub struct Chat {
    model: LlamaModel,
    /// Resolved once at load: a GGUF without a template is refused, never given a guessed format.
    template: LlamaChatTemplate,
    gate: Mutex<()>,
    /// Qwen hybrid-thinking prefill: [`EMPTY_THINK_BLOCK`] after the generation prompt. Other families pass `false`.
    disable_thinking: bool,
}

impl Chat {
    pub fn load(path: &Path, disable_thinking: bool) -> Result<Self, RuntimeError> {
        let params = LlamaModelParams::default().with_n_gpu_layers(1000);
        let model = LlamaModel::load_from_file(backend()?, path, &params).map_err(RuntimeError::engine)?;
        let template = model.chat_template(None).map_err(|error| {
            RuntimeError::engine(format!(
                "{} has no chat template, so its prompt format is unknown ({error})",
                path.display()
            ))
        })?;
        Ok(Self { model, template, gate: Mutex::new(()), disable_thinking })
    }

    /// The system and user turns in the GGUF's own template, ending where the assistant turn opens.
    fn render_prompt(&self, system: &str, user: &str) -> Result<String, RuntimeError> {
        let messages = [
            LlamaChatMessage::new("system".into(), system.into()).map_err(RuntimeError::engine)?,
            LlamaChatMessage::new("user".into(), user.into()).map_err(RuntimeError::engine)?,
        ];
        let mut prompt = self.model.apply_chat_template(&self.template, &messages, true).map_err(|error| {
            RuntimeError::engine(format!("the model's chat template could not be applied: {error}"))
        })?;
        if self.disable_thinking {
            prompt.push_str(EMPTY_THINK_BLOCK);
        }
        Ok(prompt)
    }

    /// The template's turn markers are special tokens and must be parsed. `add_special` follows the GGUF's own
    /// `add_bos_token` and `add_eos_token`: the Qwen3.5 files set neither, so they get no BOS, while families that
    /// ask for one (Llama, Gemma) get it.
    fn tokenize_prompt(&self, prompt: &str) -> Vec<LlamaToken> {
        self.model.vocab().tokenize(prompt.as_bytes(), true, true)
    }
}

impl LlmRuntime for Chat {
    fn generate(
        &self,
        system: &str,
        user: &str,
        sampling: &Sampling,
        stop: &AtomicBool,
    ) -> Result<String, RuntimeError> {
        let _one_at_a_time = self.gate.lock().map_err(|_| RuntimeError::LockPoisoned("model lock"))?;
        // A call cancelled while it waited for the gate must not start decoding.
        if stop.load(Ordering::Relaxed) {
            return Err(RuntimeError::Cancelled);
        }
        let backend = backend()?;
        let prompt = self.render_prompt(system, user)?;
        let tokens = self.tokenize_prompt(&prompt);
        let budget = output_budget(tokens.len(), sampling.max_tokens);
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

        let mut sampler = polish_sampler(sampling);
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
    fn the_output_budget_follows_the_input_and_a_cap_only_lowers_it() {
        use super::output_budget;
        assert_eq!(output_budget(10, None), 256, "a short input gets the floor");
        assert_eq!(output_budget(1000, None), 1500, "a rewrite runs about as long as the input, with headroom");
        assert_eq!(output_budget(100_000, None), 4096, "a long input gets the ceiling");
        assert_eq!(output_budget(1000, Some(300)), 300, "a cap lowers the budget");
        assert_eq!(output_budget(1000, Some(9000)), 1500, "a cap never raises it");
    }

    /// The user's parameters reach the sampler: a cap below what the reply needs cuts it off, and a chain that keeps one
    /// candidate repeats the greedy reply at any temperature.
    #[test]
    #[ignore = "requires DITO_POLISH_MODEL_DIR and a local Qwen3.5-4B GGUF"]
    fn polish_sampling_parameters_reach_the_sampler() {
        use dito_local::{LlmRuntime, RuntimeError, Sampling};
        use std::sync::atomic::AtomicBool;
        let spec = dito_local::catalog::default_catalog().into_iter().find(|s| s.id == "qwen3.5-4b-instruct").unwrap();
        let dito_local::catalog::ModelFormat::Chat { model, disable_thinking, sampling } = spec.format else {
            unreachable!()
        };
        let path = std::path::PathBuf::from(std::env::var("DITO_POLISH_MODEL_DIR").unwrap()).join(model);
        let chat = super::Chat::load(&path, disable_thinking).unwrap();
        let stop = AtomicBool::new(false);
        let system = "Rewrite the text without filler words. Reply with the rewritten text only.";
        let text = "えーと、明日の午後三時に、あの、会議をします。";

        let capped = Sampling { max_tokens: Some(2), ..sampling };
        assert!(matches!(chat.generate(system, text, &capped, &stop), Err(RuntimeError::Truncated)));

        let greedy = Sampling { temperature: 0.0, ..sampling };
        let reply = chat.generate(system, text, &greedy, &stop).unwrap();
        assert!(!reply.is_empty());
        assert_eq!(chat.generate(system, text, &greedy, &stop).unwrap(), reply, "greedy decoding repeats itself");
        let one_candidate = Sampling { temperature: 1.5, top_k: Some(1), top_p: Some(0.5), ..sampling };
        assert_eq!(
            chat.generate(system, text, &one_candidate, &stop).unwrap(),
            reply,
            "top-k and top-p are in the chain"
        );
    }

    #[test]
    #[ignore = "requires DITO_POLISH_MODEL_DIR and a local Qwen3.5-4B GGUF"]
    fn polish_reference_vocabulary_regression() {
        use dito_core::prompts::{PromptInput, Prompts, Style, build_system_prompt};
        use dito_local::LlmRuntime;
        use std::sync::atomic::AtomicBool;
        let spec = dito_local::catalog::default_catalog().into_iter().find(|s| s.id == "qwen3.5-4b-instruct").unwrap();
        let dito_local::catalog::ModelFormat::Chat { model, disable_thinking, sampling } = spec.format else {
            unreachable!()
        };
        let path = std::path::PathBuf::from(std::env::var("DITO_POLISH_MODEL_DIR").unwrap()).join(model);
        let model = super::Chat::load(&path, disable_thinking).unwrap();
        let terms = vec!["NebulaBanana".into(), "银河量子仓库".into()];
        let prompts = Prompts::default();
        let stop = AtomicBool::new(false);
        for text in
            ["嗯，明天下午三点开会。", "えーと、明日の午後三時に会議をします。", "Um, let's meet tomorrow at three."]
        {
            let system =
                build_system_prompt(&PromptInput { prompts: &prompts, style: Style::General, text, wordbook: &terms });
            let out = model.generate(&system, text, &sampling, &stop).unwrap();
            eprintln!("POLISH_REGRESSION {text}: {out}");
            assert!(!out.is_empty());
            for term in &terms {
                assert!(!out.contains(term));
            }
            assert!(!out.contains("reference_vocabulary"));
        }
    }

    /// The embedded Qwen3.5 template must reproduce the hand-written ChatML prompt these models were tuned on.
    #[test]
    #[ignore = "requires DITO_POLISH_MODEL_DIR and a local Qwen3.5-4B GGUF"]
    fn polish_chat_template_matches_legacy_chatml() {
        let spec = dito_local::catalog::default_catalog().into_iter().find(|s| s.id == "qwen3.5-4b-instruct").unwrap();
        let dito_local::catalog::ModelFormat::Chat { model, disable_thinking, .. } = spec.format else {
            unreachable!()
        };
        let path = std::path::PathBuf::from(std::env::var("DITO_POLISH_MODEL_DIR").unwrap()).join(model);
        let chat = super::Chat::load(&path, disable_thinking).unwrap();
        // Edge whitespace must survive: llama.cpp trims message content for some other families, never for ChatML.
        let system = "Rewrite the dictation.\n\nKeep the language.\n";
        let user = " えーと、明日の会議は三時です。\n";
        let prompt = chat.render_prompt(system, user).unwrap();
        let legacy = format!(
            "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n\
             <|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
        assert_eq!(prompt, legacy);
        // The old call: special tokens parsed, never a BOS.
        assert_eq!(chat.tokenize_prompt(&prompt), chat.model.vocab().tokenize(legacy.as_bytes(), false, true));
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

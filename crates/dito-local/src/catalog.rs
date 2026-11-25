//! The models Dito can run on-device. Sizes and SHA-256 come from the Hugging Face file listings.

use dito_core::config::LocalTextParams;
use dito_core::provider::{TextModelCapabilities, ThinkingCapabilities, ThinkingLevel};

pub const QWEN35_TEXT_CAPABILITIES: TextModelCapabilities = TextModelCapabilities {
    context_window_tokens: 262_144,
    max_output_tokens: None,
    recommended_output_tokens: Some(32_768),
    thinking: Some(ThinkingCapabilities { can_disable: true, levels: &[], default_level: None }),
};

pub const QWEN38_TEXT_CAPABILITIES: TextModelCapabilities = TextModelCapabilities {
    context_window_tokens: 262_144,
    max_output_tokens: None,
    recommended_output_tokens: None,
    thinking: Some(ThinkingCapabilities {
        can_disable: true,
        levels: &[ThinkingLevel::LOW, ThinkingLevel::MEDIUM, ThinkingLevel::XHIGH],
        default_level: Some("xhigh"),
    }),
};

/// Ternary Bonsai 2.5 27B shares Qwen 3.8's chat template, but its card states `low` is unsupported
/// and behaves like `xhigh`. Its window is the one its Prism worker is started with: a prompt beyond it is rejected,
/// although the model's own window is larger.
pub const BONSAI25_TEXT_CAPABILITIES: TextModelCapabilities = TextModelCapabilities {
    context_window_tokens: 32_768,
    max_output_tokens: None,
    recommended_output_tokens: None,
    thinking: Some(ThinkingCapabilities {
        can_disable: true,
        levels: &[ThinkingLevel::MEDIUM, ThinkingLevel::XHIGH],
        default_level: Some("xhigh"),
    }),
};

/// What the model's chat template writes at the top of the system turn for a thinking level. The Qwen 3.8 template
/// has a line for `xhigh` and `low`, and none for `medium`.
pub fn reasoning_instruction(model_id: &str, level: &str) -> Option<&'static str> {
    if !model_id.starts_with("qwen3.8-") && model_id != TERNARY_BONSAI_2_5_27B {
        return None;
    }
    match level {
        "xhigh" => Some(
            "Reasoning effort is set to xhigh. Please think carefully through the task, validate key assumptions,              consider plausible alternatives, and prioritize correctness, consistency, and clarity in the final answer.",
        ),
        "low" => Some(
            "Reasoning effort is set to low. Keep your thinking brief and focused, moving directly to the conclusion              without unnecessary elaboration.",
        ),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    Asr,
    Text,
    /// Voice activity detection for transcription. Never selectable on the Models page, which lists Asr and Text.
    Vad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpec {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex. `None` when the host publishes none (the size is still checked).
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub kind: ModelKind,
    pub text_capabilities: Option<TextModelCapabilities>,
    /// Millions of parameters of the original model, which orders the Models page from small to large.
    pub parameters_m: u32,
    pub quantization: Quantization,
    pub files: Vec<FileSpec>,
    pub memory_mb: u32,
    /// Names the exact file roles, so directory enumeration order never matters.
    pub format: ModelFormat,
    /// A display hint, never a restriction on input language.
    pub language_focus: Option<dito_core::i18n::Lang>,
}

/// How the shipped weights were compressed from the original model's precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantization {
    /// The name the file's publisher uses.
    pub label: &'static str,
    pub source_bits: u8,
    /// The smallest and largest bit width in the file; equal unless the conversion mixes them.
    pub bits: (u8, u8),
}

impl Quantization {
    pub const Q8_0: Self = Self { label: "Q8_0", source_bits: 16, bits: (8, 8) };
    pub const Q5_K_M: Self = Self { label: "Q5_K_M", source_bits: 16, bits: (5, 5) };
    pub const Q4_K_M: Self = Self { label: "Q4_K_M", source_bits: 16, bits: (4, 4) };
}

/// The user's parameters replace the catalog's recommended values field by field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sampling {
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    /// A cap on the generated tokens. `None` keeps the budget the runtime computes from the input.
    pub max_tokens: Option<u32>,
}

impl Sampling {
    pub const POLISH: Self = Self { temperature: 0.2, top_p: None, top_k: None, max_tokens: None };

    pub fn with_params(self, params: &LocalTextParams) -> Self {
        Self {
            temperature: params.temperature.unwrap_or(self.temperature),
            top_p: params.top_p.or(self.top_p),
            top_k: params.top_k.or(self.top_k),
            max_tokens: params.max_tokens.or(self.max_tokens),
        }
    }
}

/// Another checkpoint of an existing layout needs only a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelFormat {
    SenseVoice {
        model: &'static str,
        tokens: &'static str,
    },
    NemoTdt {
        encoder: &'static str,
        decoder: &'static str,
        joiner: &'static str,
        tokens: &'static str,
        feature_dim: i32,
    },
    QwenAsr {
        model: &'static str,
        projector: &'static str,
    },
    /// One self-describing file of any speech family transcribe.cpp runs. `hotwords` records whether
    /// that family takes a vocabulary, checked against the real weights when the entry is added.
    TranscribeCpp {
        model: &'static str,
        hotwords: bool,
    },
    NemoSpeech {
        model: &'static str,
        encoder: &'static str,
        archive: &'static str,
    },
    /// The prompt comes from the chat template embedded in the file, so a file without one fails to load.
    /// `disable_thinking` is the Qwen hybrid-thinking prefill (an empty think block after the generation prompt);
    /// `false` for other families. `sampling` fills every parameter the user leaves empty.
    Chat {
        model: &'static str,
        disable_thinking: bool,
        sampling: Sampling,
    },
    /// Process-backed PrismML llama.cpp worker for ternary PQ2_0/PTQ1_0 models until stock llama.cpp catches up.
    PrismChat {
        model: &'static str,
        archive: &'static str,
        sampling: Sampling,
    },
    /// Not loaded by `LocalEngines`: each audio source creates its own detector from the file.
    SileroVad {
        model: &'static str,
    },
}

impl ModelFormat {
    pub fn runtime(self) -> Runtime {
        match self {
            Self::SenseVoice { .. } | Self::NemoTdt { .. } | Self::SileroVad { .. } => Runtime::SherpaOnnx,
            Self::QwenAsr { .. } | Self::Chat { .. } => Runtime::LlamaCpp,
            Self::TranscribeCpp { .. } => Runtime::TranscribeCpp,
            Self::NemoSpeech { .. } => Runtime::NemoSpeech,
            Self::PrismChat { .. } => Runtime::PrismLlama,
        }
    }

    pub fn supports_hotwords(self) -> bool {
        matches!(self, Self::QwenAsr { .. } | Self::TranscribeCpp { hotwords: true, .. })
    }

    /// Speech models decode greedily and have none.
    pub fn sampling(self) -> Option<Sampling> {
        match self {
            Self::Chat { sampling, .. } | Self::PrismChat { sampling, .. } => Some(sampling),
            _ => None,
        }
    }

    pub fn files(self) -> Vec<&'static str> {
        match self {
            Self::SileroVad { model } => vec![model],
            Self::SenseVoice { model, tokens } => vec![model, tokens],
            Self::NemoTdt { encoder, decoder, joiner, tokens, .. } => vec![encoder, decoder, joiner, tokens],
            Self::QwenAsr { model, projector } => vec![model, projector],
            Self::NemoSpeech { model, encoder, archive } => vec![model, encoder, archive],
            Self::PrismChat { model, archive, .. } => vec![model, archive],
            Self::Chat { model, .. } | Self::TranscribeCpp { model, .. } => vec![model],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    SherpaOnnx,
    LlamaCpp,
    TranscribeCpp,
    NemoSpeech,
    PrismLlama,
}

impl ModelSpec {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

fn file(name: &str, url: &str, size: u64, sha256: Option<&str>) -> FileSpec {
    FileSpec { name: name.into(), url: url.into(), size, sha256: sha256.map(str::to_string) }
}

/// A Hugging Face repository at a pinned commit.
#[derive(Clone, Copy)]
struct HfRepo {
    path: &'static str,
    revision: &'static str,
}

fn hf(path: &'static str, revision: &'static str) -> HfRepo {
    HfRepo { path, revision }
}

/// A file's name, size and SHA-256.
type PinnedFile = (&'static str, u64, &'static str);

impl HfRepo {
    fn file(self, name: &str, size: u64, sha256: Option<&str>) -> FileSpec {
        let Self { path, revision } = self;
        file(name, &format!("https://huggingface.co/{path}/resolve/{revision}/{name}"), size, sha256)
    }

    fn pinned(self, (name, size, sha256): PinnedFile) -> FileSpec {
        self.file(name, size, Some(sha256))
    }
}

/// What an entry of a shared layout states for itself; the layout's constructor fills in the rest.
struct Entry {
    id: &'static str,
    name: &'static str,
    parameters_m: u32,
    memory_mb: u32,
}

fn qwen_asr(entry: Entry, repo: HfRepo, model: PinnedFile, projector: PinnedFile) -> ModelSpec {
    ModelSpec {
        id: entry.id.into(),
        name: entry.name.into(),
        kind: ModelKind::Asr,
        text_capabilities: None,
        parameters_m: entry.parameters_m,
        quantization: Quantization::Q8_0,
        files: vec![repo.pinned(model), repo.pinned(projector)],
        memory_mb: entry.memory_mb,
        format: ModelFormat::QwenAsr { model: model.0, projector: projector.0 },
        language_focus: None,
    }
}

/// Every speech family listed so far takes a vocabulary, so `hotwords` is on.
fn transcribe_cpp(entry: Entry, quantization: Quantization, repo: HfRepo, model: PinnedFile) -> ModelSpec {
    ModelSpec {
        id: entry.id.into(),
        name: entry.name.into(),
        kind: ModelKind::Asr,
        text_capabilities: None,
        parameters_m: entry.parameters_m,
        quantization,
        files: vec![repo.pinned(model)],
        memory_mb: entry.memory_mb,
        format: ModelFormat::TranscribeCpp { model: model.0, hotwords: true },
        language_focus: None,
    }
}

fn qwen_chat(
    entry: Entry,
    capabilities: TextModelCapabilities,
    quantization: Quantization,
    repo: HfRepo,
    model: PinnedFile,
) -> ModelSpec {
    ModelSpec {
        id: entry.id.into(),
        name: entry.name.into(),
        kind: ModelKind::Text,
        text_capabilities: Some(capabilities),
        parameters_m: entry.parameters_m,
        quantization,
        files: vec![repo.pinned(model)],
        memory_mb: entry.memory_mb,
        format: ModelFormat::Chat { model: model.0, disable_thinking: true, sampling: Sampling::POLISH },
        language_focus: None,
    }
}

/// Fastest and smallest, but no hotword support.
fn sensevoice_small() -> ModelSpec {
    let repo =
        hf("csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17", "2365baeacb507f821a0c8120fcee3d484dba7a07");
    let model = "model.int8.onnx";
    let tokens = "tokens.txt";
    ModelSpec {
        id: "sensevoice-small".into(),
        name: "SenseVoice Small".into(),
        parameters_m: 234,
        quantization: Quantization { label: "INT8", source_bits: 32, bits: (8, 8) },
        kind: ModelKind::Asr,
        text_capabilities: None,
        files: vec![
            repo.file(model, 239_233_841, Some("c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51")),
            repo.file(tokens, 315_894, None),
        ],
        memory_mb: 1200,
        format: ModelFormat::SenseVoice { model, tokens },
        language_focus: None,
    }
}

/// Community Apache-2.0 conversion, measured in bench/HAAS_JA.md. Not selected by default.
fn haas_ja() -> ModelSpec {
    let repo = hf("oboroge0/Haas-ja-v1.0-onnx-quantized", "7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39");
    let encoder = "encoder.quantized.onnx";
    let decoder = "decoder.onnx";
    let joiner = "joiner.onnx";
    let tokens = "tokens.txt";
    ModelSpec {
        id: "haas-ja".into(),
        name: "Haas-ja v1.0".into(),
        parameters_m: 616,
        quantization: Quantization { label: "INT4/8", source_bits: 32, bits: (4, 8) },
        kind: ModelKind::Asr,
        text_capabilities: None,
        files: vec![
            repo.file(encoder, 42261252, Some("aba291d61d3d594bc6fb8c7f90acbd980e7bc7bf6b7caaf3965ddc25e6f42fac")),
            repo.file(
                "encoder.data",
                410744064,
                Some("30befbf6aeb75383e6551f5e58de53efdb76972ddc7d0f76070c517899873586"),
            ),
            repo.file(decoder, 15661602, Some("2947cb5512699cbba753fe45fe221428aadaa9856597debbb7aabe19cc253622")),
            repo.file(joiner, 12158641, Some("8a68320e750890ff9e944a794b41ae1c42c6f439ac4c273503458a6b42adc12d")),
            repo.file(tokens, 28704, Some("ec78b13c5d9f156cf0d5fdbd6b5aec5785680f319c92630c11d5d11b14b83a84")),
        ],
        memory_mb: 1450,
        format: ModelFormat::NemoTdt { encoder, decoder, joiner, tokens, feature_dim: 128 },
        language_focus: Some(dito_core::i18n::Lang::Ja),
    }
}

/// Silero VAD v5 as published with sherpa-onnx; cuts transcription audio into utterances.
fn silero_vad() -> ModelSpec {
    let model = "silero_vad.onnx";
    ModelSpec {
        id: SILERO_VAD.into(),
        name: "Silero VAD".into(),
        parameters_m: 1,
        quantization: Quantization { label: "INT8", source_bits: 32, bits: (8, 8) },
        kind: ModelKind::Vad,
        text_capabilities: None,
        files: vec![file(
            model,
            &format!("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{model}"),
            643_854,
            Some("9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6"),
        )],
        memory_mb: 20,
        format: ModelFormat::SileroVad { model },
        language_focus: None,
    }
}

fn nemotron_transcribe() -> ModelSpec {
    let repo = hf("AtomicChat/NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-GGUF", "dbd06c5cad7a2f33c8e06be2fb3ef814dbd9aa59");
    let model = "NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-AD-IQ4_NL.gguf";
    let encoder = "NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-Encoder-F16.gguf";
    let archive = "nemo-speech-0.3.0-macos-aarch64-metal.tar.gz";
    ModelSpec {
        id: NEMOTRON_TRANSCRIBE.into(),
        name: "Nemotron 3.5 Transcribe 30B-A4B".into(),
        kind: ModelKind::Asr,
        text_capabilities: None,
        parameters_m: 34000,
        quantization: Quantization { label: "IQ4_NL", source_bits: 16, bits: (4, 32) },
        files: vec![
            repo.file(model, 19_678_656_672, Some("0c47d4ab6c783e7606ec03714b354fab0b2052027a7ea1991e7e2d3009c345ad")),
            repo.file(encoder, 1_670_410_080, Some("4221029c8a565c4a80c507ffa760c716e595acf663f9f4273ce69910050e7ba2")),
            file(
                archive,
                &format!("https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v0.3.0/{archive}"),
                7_239_335,
                Some("2b972ed8b5dba07492212acf8cb75cbf34ecb141198ffa1997febe84b7c4d9ed"),
            ),
        ],
        memory_mb: 20000,
        format: ModelFormat::NemoSpeech { model, encoder, archive },
        language_focus: None,
    }
}

/// Ternary (g128 + Hadamard) Qwen3.8-27B derivative in PQ2_0 packing, Apple Silicon's fastest ternary format.
/// Runs behind a managed PrismML llama-server worker until stock llama-cpp-sys-2 ships b11556+.
fn ternary_bonsai_2_5_27b() -> ModelSpec {
    let repo = hf("prism-ml/Ternary-Bonsai-2.5-27B-GGUF", "28a7093cc25bc151cc2b284c5570f12ce343fdcf");
    let model = "Ternary-Bonsai-2.5-27B-PQ2_0.gguf";
    let archive = "llama-prism-b12178-03ec600-bin-macos-arm64.tar.gz";
    ModelSpec {
        id: TERNARY_BONSAI_2_5_27B.into(),
        name: "Ternary Bonsai 2.5 27B".into(),
        kind: ModelKind::Text,
        text_capabilities: Some(BONSAI25_TEXT_CAPABILITIES),
        parameters_m: 27000,
        quantization: Quantization { label: "PQ2_0", source_bits: 16, bits: (2, 2) },
        files: vec![
            repo.file(model, 7_253_354_848, Some("103a11c94324029d8bae3f8152f30bba414730064cfb77438f8589db72e1aa85")),
            file(
                archive,
                &format!("https://github.com/PrismML-Eng/llama.cpp/releases/download/prism-b12178-03ec600/{archive}"),
                13_209_869,
                Some("94d791a4ad9d6ea0c9884929830352397af8274ca107afab497b1de0147bf99b"),
            ),
        ],
        memory_mb: 10000,
        format: ModelFormat::PrismChat { model, archive, sampling: Sampling::POLISH },
        language_focus: None,
    }
}

pub fn default_catalog() -> Vec<ModelSpec> {
    let mut catalog = vec![
        // Measured on M4 Pro (bench, 2026-11): 0.4-0.6 s per clip, the most accurate tier and
        // the default; same hotword/context path as the 0.6B. Official repo ships Q8_0 only (no Q4).
        qwen_asr(
            Entry { id: "qwen3-asr-1.7b", name: "Qwen3-ASR-1.7B", parameters_m: 2349, memory_mb: 3100 },
            hf("ggml-org/Qwen3-ASR-1.7B-GGUF", "36a678687ba7d07a74ca70ccb0e36902e005fb80"),
            (
                "Qwen3-ASR-1.7B-Q8_0.gguf",
                2_165_034_944,
                "58e22d0532d4eacaf034cfac17a6fed159f37c41390c710186783be439d1fc57",
            ),
            (
                "mmproj-Qwen3-ASR-1.7B-Q8_0.gguf",
                355_709_344,
                "46c1d533af3f354ceb37ce855dbceff7da7fa7cf1e6a523df3b13440bd164c0d",
            ),
        ),
        // Measured on M4 Pro (bench, 2026-11): 0.15-0.24 s per dictation clip, ~90x realtime;
        // hotword (context) support. The long-audio defect (llama.cpp #21847, pre-2026-05) is fixed
        // in the vendored llama.cpp: a 336.7 s clip transcribes in full (CER 0.003, 9.6 s).
        qwen_asr(
            Entry { id: "qwen3-asr-0.6b", name: "Qwen3-ASR-0.6B", parameters_m: 938, memory_mb: 1250 },
            hf("ggml-org/Qwen3-ASR-0.6B-GGUF", "928ab958557df9aa2ef1c93e0e83c7ad0933fae2"),
            (
                "Qwen3-ASR-0.6B-Q8_0.gguf",
                804_749_248,
                "bca259818b50ca7c4c05e9bdb35a5dc04fa039653a6d6f3f0f331f96f6aa1971",
            ),
            (
                "mmproj-Qwen3-ASR-0.6B-Q8_0.gguf",
                214_392_480,
                "41a342b5e4c514e968cb756de6cd1b7be39eff43c44c57a2ef5fc6522e36603d",
            ),
        ),
        sensevoice_small(),
        haas_ja(),
        // The vendor's own GGUF of a Qwen3-ASR fine-tune, so it shares the 1.7B prompt and hotword path.
        // NetEase Youdao model license, not Apache-2.0.
        qwen_asr(
            Entry { id: "confucius4-r2t2", name: "Confucius4-R2T2", parameters_m: 2038, memory_mb: 3100 },
            hf("netease-youdao/Confucius4-R2T2-GGUF", "86ff0251cb9f456b63aeef5f80137f104e22869a"),
            (
                "Confucius4-R2T2-Q8_0.gguf",
                1_834_422_208,
                "151097e43957a19984ea7de66e8144ce69b95039eb31c93da4f58db367e455c3",
            ),
            (
                "mmproj-Confucius4-R2T2-Q8_0.gguf",
                348_336_544,
                "8dc2c67e6a0484114928142d098db7ad94ae9f34c78948ef9d37a9678418cb65",
            ),
        ),
        // The transcribe.cpp project's GGUF of openai/whisper-large-v3 (Apache-2.0).
        transcribe_cpp(
            Entry { id: "whisper-large-v3", name: "whisper-large-v3", parameters_m: 1543, memory_mb: 2000 },
            Quantization::Q5_K_M,
            hf("handy-computer/whisper-large-v3-gguf", "274ad1d1cf680b7ee53cc70f78461394cd3a344b"),
            (
                "whisper-large-v3-Q5_K_M.gguf",
                1_161_143_008,
                "6053d0fd69a0fd48b8fea5ea7a52b9e0cde389343566fa30e453a1b2b258dc38",
            ),
        ),
        // GGUF of FunAudioLLM/Fun-ASR-MLT-Nano-2512: a SenseVoice encoder in front of Qwen3-0.6B.
        transcribe_cpp(
            Entry { id: "fun-asr-mlt-nano-2512", name: "Fun-ASR-MLT-Nano-2512", parameters_m: 800, memory_mb: 1500 },
            Quantization::Q8_0,
            hf("xix3r/voxix-Fun-ASR-MLT-Nano-2512-gguf", "38f895a0884f6e2ceb037277d22bc003a5a67f87"),
            (
                "Fun-ASR-MLT-Nano-2512-Q8_0.gguf",
                891_271_232,
                "d12476d8d9f2baa0ebf738fa955fa05ed33a654f1567289033a810c45d9d9002",
            ),
        ),
        // The transcribe.cpp project's GGUF of openai/whisper-large-v3-turbo (MIT): the large-v3 encoder
        // with a four-layer decoder.
        transcribe_cpp(
            Entry { id: "whisper-large-v3-turbo", name: "whisper-large-v3-turbo", parameters_m: 809, memory_mb: 1200 },
            Quantization::Q5_K_M,
            hf("handy-computer/whisper-large-v3-turbo-gguf", "18ada752005f5fddef499256f9a458d0b37a203a"),
            (
                "whisper-large-v3-turbo-Q5_K_M.gguf",
                619_628_096,
                "06fc9923c09fb3ae878ea12c34d1be3ed18fad039ab14b4b045c7b6c40af56b2",
            ),
        ),
        qwen_chat(
            Entry { id: "qwen3.5-2b-instruct", name: "Qwen3.5-2B-Instruct", parameters_m: 2000, memory_mb: 1600 },
            QWEN35_TEXT_CAPABILITIES,
            Quantization::Q4_K_M,
            hf("unsloth/Qwen3.5-2B-GGUF", "f6d5376be1edb4d416d56da11e5397a961aca8ae"),
            (
                "Qwen3.5-2B-Q4_K_M.gguf",
                1_280_835_840,
                "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
            ),
        ),
        qwen_chat(
            Entry { id: "qwen3.5-4b-instruct", name: "Qwen3.5-4B-Instruct", parameters_m: 4000, memory_mb: 3400 },
            QWEN35_TEXT_CAPABILITIES,
            Quantization::Q4_K_M,
            hf("unsloth/Qwen3.5-4B-GGUF", "e87f176479d0855a907a41277aca2f8ee7a09523"),
            (
                "Qwen3.5-4B-Q4_K_M.gguf",
                2_740_937_888,
                "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
            ),
        ),
        // Measured on M4 Pro (bench, 2026-11): 9B is the only tier where ASR-hotword →
        // polish-wordbook injection stays stable (4B drops entries); 2B keeps English
        // terms at 0.25–0.32 s latency.
        qwen_chat(
            Entry { id: "qwen3.5-9b-instruct", name: "Qwen3.5-9B-Instruct", parameters_m: 9000, memory_mb: 7000 },
            QWEN35_TEXT_CAPABILITIES,
            Quantization::Q4_K_M,
            hf("unsloth/Qwen3.5-9B-GGUF", "3885219b6810b007914f3a7950a8d1b469d598a5"),
            (
                "Qwen3.5-9B-Q4_K_M.gguf",
                5_680_522_464,
                "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
            ),
        ),
        qwen_chat(
            Entry { id: "qwen3.8-27b-instruct", name: "Qwen3.8-27B", parameters_m: 27000, memory_mb: 20000 },
            QWEN38_TEXT_CAPABILITIES,
            Quantization { label: "UD-Q4_K_M", source_bits: 16, bits: (3, 8) },
            hf("unsloth/Qwen3.8-27B-GGUF", "4af51b3542f775b0a8e4b310bfb3f17e01c306c9"),
            (
                "Qwen3.8-27B-UD-Q4_K_M.gguf",
                16_564_259_744,
                "1bce504dfecd84153666928a41d8a66706f0158d3a317bc5196b72819d7dc529",
            ),
        ),
        silero_vad(),
    ];
    if cfg!(target_arch = "aarch64") {
        catalog.push(nemotron_transcribe());
        catalog.push(ternary_bonsai_2_5_27b());
    }
    // The Models page lists each kind in this order.
    catalog.sort_by_key(|model| model.parameters_m);
    catalog
}

/// The voice activity detector transcription downloads on first use.
pub const SILERO_VAD: &str = "silero-vad";

pub const NEMOTRON_TRANSCRIBE: &str = "nemotron-3.5-transcribe";

pub const TERNARY_BONSAI_2_5_27B: &str = "ternary-bonsai-2.5-27b";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_defaults_exist() {
        let c = default_catalog();
        let mut ids: Vec<_> = c.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.len());
        assert!(c.iter().any(|m| m.id == dito_core::config::DEFAULT_ASR_LOCAL));
        assert!(c.iter().any(|m| m.id == dito_core::config::DEFAULT_TEXT_LOCAL));
    }

    #[test]
    fn every_large_file_has_a_checksum() {
        let unverified: Vec<_> = default_catalog()
            .iter()
            .flat_map(|m| m.files.iter())
            .filter(|f| f.size >= 1_000_000 && f.sha256.as_deref().is_none_or(|h| h.len() != 64))
            .map(|f| f.name.clone())
            .collect();
        assert!(unverified.is_empty(), "{unverified:?}");
    }

    #[test]
    fn every_huggingface_url_is_pinned_to_a_commit() {
        let unpinned: Vec<_> = default_catalog()
            .iter()
            .flat_map(|m| m.files.iter())
            .filter(|f| !crate::manifest::is_pinned_hf_or_non_hf_url(&f.url))
            .map(|f| f.url.clone())
            .collect();
        assert!(unpinned.is_empty(), "Hugging Face URLs must pin a 40-char commit SHA: {unpinned:?}");
    }
    #[test]
    fn user_parameters_replace_the_recommended_sampling_field_by_field() {
        let recommended = Sampling { temperature: 0.2, top_p: Some(0.9), top_k: None, max_tokens: None };
        assert_eq!(recommended.with_params(&LocalTextParams::default()), recommended, "nothing set, nothing changed");
        let set = LocalTextParams { temperature: Some(0.7), top_p: None, top_k: Some(40), max_tokens: Some(100) };
        let expected = Sampling { temperature: 0.7, top_p: Some(0.9), top_k: Some(40), max_tokens: Some(100) };
        assert_eq!(recommended.with_params(&set), expected, "an unset field keeps the recommendation");
    }

    #[test]
    fn catalog_layouts_reference_declared_files_and_match_the_task() {
        for spec in default_catalog() {
            assert_eq!(spec.text_capabilities.is_some(), spec.kind == ModelKind::Text);
            for name in spec.format.files() {
                assert!(spec.files.iter().any(|f| f.name == name), "{}: missing {name}", spec.id);
            }
            assert_eq!(
                spec.kind == ModelKind::Text,
                matches!(spec.format, ModelFormat::Chat { .. } | ModelFormat::PrismChat { .. })
            );
            assert_eq!(spec.format.sampling().is_some(), spec.kind == ModelKind::Text, "{}", spec.id);
            assert_eq!(spec.kind == ModelKind::Vad, matches!(spec.format, ModelFormat::SileroVad { .. }));
        }
        let haas = default_catalog().into_iter().find(|s| s.id == "haas-ja").unwrap();
        assert!(haas.files.iter().any(|f| f.name == "encoder.data" && f.sha256.is_some()));
        assert!(!haas.format.supports_hotwords());
        assert_eq!(haas.format.runtime(), Runtime::SherpaOnnx);
        assert_ne!(haas.id, dito_core::config::DEFAULT_ASR_LOCAL);
        let qwen = default_catalog().into_iter().find(|s| s.id == "qwen3-asr-0.6b").unwrap();
        assert!(qwen.format.supports_hotwords());
    }
}

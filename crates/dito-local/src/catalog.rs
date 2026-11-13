//! The models Dito can run on-device. Sizes and SHA-256 come from the Hugging Face file listings.

use dito_core::config::LocalPolishParams;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    Asr,
    Polish,
    /// Voice activity detection for transcription. Never selectable on the Models page, which lists Asr and Polish.
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
    pub files: Vec<FileSpec>,
    pub memory_mb: u32,
    /// Names the exact file roles, so directory enumeration order never matters.
    pub format: ModelFormat,
    /// A display hint, never a restriction on input language.
    pub language_focus: Option<dito_core::i18n::Lang>,
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

    pub fn with_params(self, params: &LocalPolishParams) -> Self {
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
    /// The prompt comes from the chat template embedded in the file, so a file without one fails to load.
    /// `disable_thinking` is the Qwen hybrid-thinking prefill (an empty think block after the generation prompt);
    /// `false` for other families. `sampling` fills every parameter the user leaves empty.
    Chat {
        model: &'static str,
        disable_thinking: bool,
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
        }
    }

    pub fn supports_hotwords(self) -> bool {
        matches!(self, Self::QwenAsr { .. })
    }

    /// Speech models decode greedily and have none.
    pub fn sampling(self) -> Option<Sampling> {
        match self {
            Self::Chat { sampling, .. } => Some(sampling),
            Self::SenseVoice { .. } | Self::NemoTdt { .. } | Self::QwenAsr { .. } | Self::SileroVad { .. } => None,
        }
    }

    pub fn files(self) -> Vec<&'static str> {
        match self {
            Self::SileroVad { model } => vec![model],
            Self::SenseVoice { model, tokens } => vec![model, tokens],
            Self::NemoTdt { encoder, decoder, joiner, tokens, .. } => vec![encoder, decoder, joiner, tokens],
            Self::QwenAsr { model, projector } => vec![model, projector],
            Self::Chat { model, .. } => vec![model],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    SherpaOnnx,
    LlamaCpp,
}

impl ModelSpec {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

fn file(name: &str, url: &str, size: u64, sha256: Option<&str>) -> FileSpec {
    FileSpec { name: name.into(), url: url.into(), size, sha256: sha256.map(str::to_string) }
}

pub fn default_catalog() -> Vec<ModelSpec> {
    let hf = "https://huggingface.co";
    // Display order within a kind is the order here: the Models page lists them as written.
    vec![
        // Measured on M4 Pro (bench, 2026-11): 0.4-0.6 s per clip, the most accurate tier and
        // the default; same hotword/context path as the 0.6B. Official repo ships Q8_0 only (no Q4).
        ModelSpec {
            id: "qwen3-asr-1.7b".into(),
            name: "Qwen3-ASR-1.7B".into(),
            kind: ModelKind::Asr,
            files: vec![
                file(
                    "Qwen3-ASR-1.7B-Q8_0.gguf",
                    &format!("{hf}/ggml-org/Qwen3-ASR-1.7B-GGUF/resolve/main/Qwen3-ASR-1.7B-Q8_0.gguf"),
                    2_165_034_944,
                    Some("58e22d0532d4eacaf034cfac17a6fed159f37c41390c710186783be439d1fc57"),
                ),
                file(
                    "mmproj-Qwen3-ASR-1.7B-Q8_0.gguf",
                    &format!("{hf}/ggml-org/Qwen3-ASR-1.7B-GGUF/resolve/main/mmproj-Qwen3-ASR-1.7B-Q8_0.gguf"),
                    355_709_344,
                    Some("46c1d533af3f354ceb37ce855dbceff7da7fa7cf1e6a523df3b13440bd164c0d"),
                ),
            ],
            memory_mb: 3100,
            format: ModelFormat::QwenAsr {
                model: "Qwen3-ASR-1.7B-Q8_0.gguf",
                projector: "mmproj-Qwen3-ASR-1.7B-Q8_0.gguf",
            },
            language_focus: None,
        },
        // Measured on M4 Pro (bench, 2026-11): 0.15-0.24 s per dictation clip, ~90x realtime;
        // hotword (context) support. The long-audio defect (llama.cpp #21847, pre-2026-05) is fixed
        // in the vendored llama.cpp: a 336.7 s clip transcribes in full (CER 0.003, 9.6 s).
        ModelSpec {
            id: "qwen3-asr-0.6b".into(),
            name: "Qwen3-ASR-0.6B".into(),
            kind: ModelKind::Asr,
            files: vec![
                file(
                    "Qwen3-ASR-0.6B-Q8_0.gguf",
                    &format!("{hf}/ggml-org/Qwen3-ASR-0.6B-GGUF/resolve/main/Qwen3-ASR-0.6B-Q8_0.gguf"),
                    804_749_248,
                    Some("bca259818b50ca7c4c05e9bdb35a5dc04fa039653a6d6f3f0f331f96f6aa1971"),
                ),
                file(
                    "mmproj-Qwen3-ASR-0.6B-Q8_0.gguf",
                    &format!("{hf}/ggml-org/Qwen3-ASR-0.6B-GGUF/resolve/main/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf"),
                    214_392_480,
                    Some("41a342b5e4c514e968cb756de6cd1b7be39eff43c44c57a2ef5fc6522e36603d"),
                ),
            ],
            memory_mb: 1250,
            format: ModelFormat::QwenAsr {
                model: "Qwen3-ASR-0.6B-Q8_0.gguf",
                projector: "mmproj-Qwen3-ASR-0.6B-Q8_0.gguf",
            },
            language_focus: None,
        },
        // Fastest and smallest, but no hotword support.
        ModelSpec {
            id: "sensevoice-small".into(),
            name: "SenseVoice Small".into(),
            kind: ModelKind::Asr,
            files: vec![
                file(
                    "model.int8.onnx",
                    &format!(
                        "{hf}/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx"
                    ),
                    239_233_841,
                    Some("c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51"),
                ),
                file(
                    "tokens.txt",
                    &format!(
                        "{hf}/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt"
                    ),
                    315_894,
                    None,
                ),
            ],
            memory_mb: 1200,
            format: ModelFormat::SenseVoice { model: "model.int8.onnx", tokens: "tokens.txt" },
            language_focus: None,
        },
        // Community Apache-2.0 conversion, measured in bench/HAAS_JA.md. Not selected by default.
        ModelSpec {
            id: "haas-ja".into(),
            name: "Haas-ja v1.0".into(),
            kind: ModelKind::Asr,
            files: vec![
                file(
                    "encoder.quantized.onnx",
                    &format!(
                        "{hf}/oboroge0/Haas-ja-v1.0-onnx-quantized/resolve/7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39/encoder.quantized.onnx"
                    ),
                    42261252,
                    Some("aba291d61d3d594bc6fb8c7f90acbd980e7bc7bf6b7caaf3965ddc25e6f42fac"),
                ),
                file(
                    "encoder.data",
                    &format!(
                        "{hf}/oboroge0/Haas-ja-v1.0-onnx-quantized/resolve/7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39/encoder.data"
                    ),
                    410744064,
                    Some("30befbf6aeb75383e6551f5e58de53efdb76972ddc7d0f76070c517899873586"),
                ),
                file(
                    "decoder.onnx",
                    &format!(
                        "{hf}/oboroge0/Haas-ja-v1.0-onnx-quantized/resolve/7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39/decoder.onnx"
                    ),
                    15661602,
                    Some("2947cb5512699cbba753fe45fe221428aadaa9856597debbb7aabe19cc253622"),
                ),
                file(
                    "joiner.onnx",
                    &format!(
                        "{hf}/oboroge0/Haas-ja-v1.0-onnx-quantized/resolve/7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39/joiner.onnx"
                    ),
                    12158641,
                    Some("8a68320e750890ff9e944a794b41ae1c42c6f439ac4c273503458a6b42adc12d"),
                ),
                file(
                    "tokens.txt",
                    &format!(
                        "{hf}/oboroge0/Haas-ja-v1.0-onnx-quantized/resolve/7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39/tokens.txt"
                    ),
                    28704,
                    Some("ec78b13c5d9f156cf0d5fdbd6b5aec5785680f319c92630c11d5d11b14b83a84"),
                ),
            ],
            memory_mb: 1450,
            format: ModelFormat::NemoTdt {
                encoder: "encoder.quantized.onnx",
                decoder: "decoder.onnx",
                joiner: "joiner.onnx",
                tokens: "tokens.txt",
                feature_dim: 128,
            },
            language_focus: Some(dito_core::i18n::Lang::Ja),
        },
        ModelSpec {
            id: "qwen3.5-2b-instruct".into(),
            name: "Qwen3.5-2B-Instruct".into(),
            kind: ModelKind::Polish,
            files: vec![file(
                "Qwen3.5-2B-Q4_K_M.gguf",
                &format!("{hf}/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf"),
                1_280_835_840,
                Some("aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223"),
            )],
            memory_mb: 1600,
            format: ModelFormat::Chat {
                model: "Qwen3.5-2B-Q4_K_M.gguf",
                disable_thinking: true,
                sampling: Sampling::POLISH,
            },
            language_focus: None,
        },
        ModelSpec {
            id: "qwen3.5-4b-instruct".into(),
            name: "Qwen3.5-4B-Instruct".into(),
            kind: ModelKind::Polish,
            files: vec![file(
                "Qwen3.5-4B-Q4_K_M.gguf",
                &format!("{hf}/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"),
                2_740_937_888,
                Some("00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4"),
            )],
            memory_mb: 3400,
            format: ModelFormat::Chat {
                model: "Qwen3.5-4B-Q4_K_M.gguf",
                disable_thinking: true,
                sampling: Sampling::POLISH,
            },
            language_focus: None,
        },
        // Measured on M4 Pro (bench, 2026-11): 9B is the only tier where ASR-hotword →
        // polish-wordbook injection stays stable (4B drops entries); 2B keeps English
        // terms at 0.25–0.32 s latency.
        ModelSpec {
            id: "qwen3.5-9b-instruct".into(),
            name: "Qwen3.5-9B-Instruct".into(),
            kind: ModelKind::Polish,
            files: vec![file(
                "Qwen3.5-9B-Q4_K_M.gguf",
                &format!("{hf}/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q4_K_M.gguf"),
                5_680_522_464,
                Some("03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8"),
            )],
            memory_mb: 7000,
            format: ModelFormat::Chat {
                model: "Qwen3.5-9B-Q4_K_M.gguf",
                disable_thinking: true,
                sampling: Sampling::POLISH,
            },
            language_focus: None,
        },
        // Silero VAD v5 as published with sherpa-onnx; cuts transcription audio into utterances.
        ModelSpec {
            id: SILERO_VAD.into(),
            name: "Silero VAD".into(),
            kind: ModelKind::Vad,
            files: vec![file(
                "silero_vad.onnx",
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
                643_854,
                Some("9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6"),
            )],
            memory_mb: 20,
            format: ModelFormat::SileroVad { model: "silero_vad.onnx" },
            language_focus: None,
        },
    ]
}

/// The voice activity detector transcription downloads on first use.
pub const SILERO_VAD: &str = "silero-vad";

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
        assert!(c.iter().any(|m| m.id == dito_core::config::DEFAULT_POLISH_LOCAL));
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
    fn user_parameters_replace_the_recommended_sampling_field_by_field() {
        let recommended = Sampling { temperature: 0.2, top_p: Some(0.9), top_k: None, max_tokens: None };
        assert_eq!(recommended.with_params(&LocalPolishParams::default()), recommended, "nothing set, nothing changed");
        let set = LocalPolishParams { temperature: Some(0.7), top_p: None, top_k: Some(40), max_tokens: Some(100) };
        let expected = Sampling { temperature: 0.7, top_p: Some(0.9), top_k: Some(40), max_tokens: Some(100) };
        assert_eq!(recommended.with_params(&set), expected, "an unset field keeps the recommendation");
    }

    #[test]
    fn catalog_layouts_reference_declared_files_and_match_the_task() {
        for spec in default_catalog() {
            for name in spec.format.files() {
                assert!(spec.files.iter().any(|f| f.name == name), "{}: missing {name}", spec.id);
            }
            assert_eq!(spec.kind == ModelKind::Polish, matches!(spec.format, ModelFormat::Chat { .. }));
            assert_eq!(spec.format.sampling().is_some(), spec.kind == ModelKind::Polish, "{}", spec.id);
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

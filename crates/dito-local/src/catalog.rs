//! The models Dito can run on-device. Sizes and SHA-256 come from the Hugging Face file listings.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    Asr,
    Polish,
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
    /// Typical resident memory while loaded.
    pub memory_mb: u32,
    /// Inference runtime that understands the files.
    pub runtime: Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// sherpa-onnx (SenseVoice).
    SherpaOnnx,
    /// llama.cpp GGUF files.
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
            runtime: Runtime::LlamaCpp,
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
            runtime: Runtime::LlamaCpp,
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
            runtime: Runtime::SherpaOnnx,
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
            runtime: Runtime::LlamaCpp,
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
            runtime: Runtime::LlamaCpp,
        },
        // Measured on M4 Pro (bench, 2026-11): 9B is the only tier where ASR-hotword →
        // polish-dictionary injection stays stable (4B drops entries); 2B keeps English
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
            runtime: Runtime::LlamaCpp,
        },
    ]
}

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
        for m in default_catalog() {
            for f in &m.files {
                assert!(
                    f.size < 1_000_000 || f.sha256.as_deref().is_some_and(|h| h.len() == 64),
                    "{} / {}",
                    m.id,
                    f.name
                );
            }
        }
    }
}

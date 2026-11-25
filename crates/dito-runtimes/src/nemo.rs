use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dito_core::audio::AudioClip;
use dito_local::{AsrOutput, AsrRuntime, RuntimeError};
use serde_json::{Value, json};

use crate::worker::{Runtime, Worker, block_on, cancelled, client};

const RUNTIME: Runtime = Runtime {
    label: "NeMo",
    lock_file: ".nemo-runtime.lock",
    install_dir: ".nemo-runtime-0.3.0",
    executable: "nemo-speech-0.3.0-macos-aarch64-metal/bin/nemo-speech",
    sha256: "2b972ed8b5dba07492212acf8cb75cbf34ecb141198ffa1997febe84b7c4d9ed",
};

pub struct Speech {
    worker: Worker,
}

impl Speech {
    pub fn load(
        dir: &Path,
        model: &str,
        encoder: &str,
        archive: &str,
        stop: &AtomicBool,
    ) -> Result<Self, RuntimeError> {
        cancelled(stop)?;
        let model = super::model_file(dir, model)?;
        let encoder = super::model_file(dir, encoder)?;
        let binary = RUNTIME.install(dir, archive, stop)?;
        cancelled(stop)?;
        let worker = Worker::spawn(&binary, &dir.join("nemo-speech.log"), |port, token| {
            let port = port.to_string();
            [
                "serve",
                "--asr-model",
                &model.to_string_lossy(),
                "--salm-encoder",
                &encoder.to_string_lossy(),
                "--host",
                "127.0.0.1",
                "--port",
                &port,
                "--api-key",
                token,
                "--device",
                "metal",
                "--threads",
                "4",
                "--no-ui",
                "--no-warmup",
                "--asr.salm.max_sequences",
                "1",
                "--asr.salm.n_ctx_per_sequence",
                "73728",
                "--asr.salm.max_context",
                "73728",
                "--asr.salm.max_new_tokens",
                "2048",
                "--asr.salm.n_batch",
                "512",
                "--asr.salm.encoder_max_batch",
                "1",
                "--asr.salm.mtp_draft",
                "0",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        })?;
        let engine = Self { worker };
        engine.worker.wait_ready("NeMo", "nemo-speech.log", "/v1/models", stop, |body| {
            body.and_then(|body| body["data"].as_array())
                .is_some_and(|models| models.iter().any(|model| model["id"] == "salm"))
        })?;
        cancelled(stop)?;
        Ok(engine)
    }
}

fn instruction(language: Option<&str>) -> String {
    let script = match language {
        Some("ja") => "Write Japanese in Japanese script and English in Latin script. ",
        Some("zh") => "Write Chinese in simplified Chinese and English in Latin script. ",
        _ => "Write each language in its native script and English in Latin script. ",
    };
    format!(
        "Transcribe verbatim in the original languages. Preserve hesitations and false starts. {script}Return only the transcript with punctuation and capitalization."
    )
}

impl AsrRuntime for Speech {
    fn shutdown(&self) {
        self.worker.shutdown();
    }

    fn failure(&self) -> Option<String> {
        self.worker.failure()
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.worker.pid())
    }

    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        _: &[String],
        stop: &AtomicBool,
    ) -> Result<AsrOutput, RuntimeError> {
        cancelled(stop)?;
        if let Some(error) = self.failure() {
            return Err(RuntimeError::engine(error));
        }
        // SALM reads the whole recording; splitting speech at a fixed boundary can lose words.
        {
            let wav = AudioClip::new(samples.to_vec()).to_wav();
            let mut body = json!({
                "model": "salm", "temperature": 0, "stream": false,
                "messages": [{"role": "user", "content": [
                    {"type": "text", "text": instruction(language)},
                    {"type": "input_audio", "input_audio": {"data": STANDARD.encode(wav), "format": "wav"}}
                ]}]
            });
            if let Some(language) = language {
                body["language"] = language.into();
            }
            let value = block_on(async {
                let client = client(Duration::from_secs(900))?;
                self.worker.post_chat(&client, &body, stop).await
            })?;
            cancelled(stop)?;
            if value.pointer("/choices/0/finish_reason").and_then(Value::as_str) != Some("stop") {
                return Err(RuntimeError::Truncated);
            }
            let output = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .ok_or_else(|| RuntimeError::engine("NeMo returned no transcript"))?;
            Ok(AsrOutput { text: output.trim().to_owned(), language: language.map(str::to_owned) })
        }
    }
}

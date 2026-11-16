use std::future::Future;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dito_core::audio::AudioClip;
use dito_local::managed_process::ManagedProcess;
use dito_local::{AsrOutput, AsrRuntime, RuntimeError};
use serde_json::{Value, json};

const ARCHIVE_SHA: &str = "2b972ed8b5dba07492212acf8cb75cbf34ecb141198ffa1997febe84b7c4d9ed";
const PACKAGE: &str = "nemo-speech-0.3.0-macos-aarch64-metal";

pub struct Speech {
    process: ManagedProcess,
    endpoint: String,
    token: String,
}

fn cancelled(stop: &AtomicBool) -> Result<(), RuntimeError> {
    if stop.load(Ordering::Relaxed) { Err(RuntimeError::Cancelled) } else { Ok(()) }
}

fn block_on<T>(future: impl Future<Output = Result<T, RuntimeError>>) -> Result<T, RuntimeError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(RuntimeError::engine)?.block_on(future)
}

fn client(timeout: Duration) -> Result<reqwest::Client, RuntimeError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(timeout)
        .build()
        .map_err(RuntimeError::engine)
}

fn install(dir: &Path, archive: &str, stop: &AtomicBool) -> Result<PathBuf, RuntimeError> {
    let lock = std::fs::File::options()
        .create(true)
        .append(true)
        .open(dir.join(".nemo-runtime.lock"))
        .map_err(RuntimeError::engine)?;
    loop {
        cancelled(stop)?;
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(RuntimeError::engine(error)),
        }
    }
    let destination = dir.join(".nemo-runtime-0.3.0");
    let binary = destination.join(PACKAGE).join("bin/nemo-speech");
    if binary.is_file() {
        return Ok(binary);
    }
    let archive = super::model_file(dir, archive)?;
    let digest = block_on(async { dito_local::download::sha256_file(&archive).await.map_err(RuntimeError::engine) })?;
    if digest != ARCHIVE_SHA {
        return Err(RuntimeError::engine("NeMo runtime archive checksum mismatch"));
    }
    cancelled(stop)?;
    let temporary = dir.join(".nemo-runtime-0.3.0.installing");
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary).map_err(RuntimeError::engine)?;
    }
    std::fs::create_dir(&temporary).map_err(RuntimeError::engine)?;
    let output = std::process::Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&temporary)
        .output()
        .map_err(RuntimeError::engine)?;
    if !output.status.success() {
        return Err(RuntimeError::engine("could not unpack NeMo runtime"));
    }
    if !temporary.join(PACKAGE).join("bin/nemo-speech").is_file() {
        return Err(RuntimeError::engine("NeMo runtime package has no executable"));
    }
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(RuntimeError::engine)?;
    }
    std::fs::rename(&temporary, &destination).map_err(RuntimeError::engine)?;
    Ok(binary)
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
        let binary = install(dir, archive, stop)?;
        cancelled(stop)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(RuntimeError::engine)?;
        let port = listener.local_addr().map_err(RuntimeError::engine)?.port();
        let mut secret = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut secret))
            .map_err(RuntimeError::engine)?;
        let token = STANDARD.encode(secret);
        let args: Vec<String> = [
            "serve",
            "--asr-model",
            &model.to_string_lossy(),
            "--salm-encoder",
            &encoder.to_string_lossy(),
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            &token,
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
            "512",
            "--asr.salm.n_batch",
            "512",
            "--asr.salm.encoder_max_batch",
            "1",
            "--asr.salm.mtp_draft",
            "0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        drop(listener);
        let log = std::fs::File::create(dir.join("nemo-speech.log")).map_err(RuntimeError::engine)?;
        let process = ManagedProcess::spawn(&binary, &args, log)?;
        let engine = Self { process, endpoint: format!("http://127.0.0.1:{port}"), token };
        block_on(async {
            let client = client(Duration::from_secs(2))?;
            let deadline = Instant::now() + Duration::from_secs(180);
            loop {
                cancelled(stop)?;
                if let Some(error) = engine.failure() {
                    return Err(RuntimeError::engine(format!("{error}; see nemo-speech.log")));
                }
                if Instant::now() >= deadline {
                    return Err(RuntimeError::engine("NeMo model startup timed out; see nemo-speech.log"));
                }
                let probe = client.get(format!("{}/v1/models", engine.endpoint)).bearer_auth(&engine.token).send();
                tokio::select! {
                    response = probe => {
                        if let Ok(response) = response
                            && response.status().is_success()
                            && let Ok(body) = response.json::<Value>().await
                            && body["data"].as_array().is_some_and(|models| models.iter().any(|model| model["id"] == "salm"))
                        {
                            return Ok(());
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
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
        self.process.shutdown();
    }

    fn failure(&self) -> Option<String> {
        self.process.failure()
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.process.pid())
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
                let request = async {
                    let response = client
                        .post(format!("{}/v1/chat/completions", self.endpoint))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await
                        .and_then(reqwest::Response::error_for_status)
                        .map_err(RuntimeError::engine)?;
                    response.json::<Value>().await.map_err(RuntimeError::engine)
                };
                tokio::pin!(request);
                loop {
                    cancelled(stop)?;
                    if let Some(error) = self.failure() {
                        return Err(RuntimeError::engine(error));
                    }
                    tokio::select! {
                        value = &mut request => return value,
                        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                    }
                }
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

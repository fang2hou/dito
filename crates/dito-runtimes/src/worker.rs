//! What the managed worker processes share: a pinned runtime archive unpacked once next to the models, a server on a
//! loopback port behind a random token, and requests that end when the run stops or the process fails.

use std::future::Future;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dito_local::RuntimeError;
use dito_local::managed_process::ManagedProcess;
use serde_json::Value;

const STARTUP: Duration = Duration::from_secs(180);
const PROBE_GAP: Duration = Duration::from_millis(100);
const WATCH: Duration = Duration::from_millis(50);

pub(crate) fn cancelled(stop: &AtomicBool) -> Result<(), RuntimeError> {
    if stop.load(Ordering::Relaxed) { Err(RuntimeError::Cancelled) } else { Ok(()) }
}

pub(crate) fn block_on<T>(future: impl Future<Output = Result<T, RuntimeError>>) -> Result<T, RuntimeError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(RuntimeError::engine)?.block_on(future)
}

pub(crate) fn client(timeout: Duration) -> Result<reqwest::Client, RuntimeError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(timeout)
        .build()
        .map_err(RuntimeError::engine)
}

/// The most characters of a rejected answer that an error keeps.
const ERROR_CHARS: usize = 400;

/// Passes an accepted answer through. A rejected one becomes an error that keeps the server's own reason, which says
/// for example that a prompt is longer than the context.
pub(crate) async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response, RuntimeError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(rejected(status.as_u16(), &response.text().await.unwrap_or_default()))
}

fn rejected(status: u16, body: &str) -> RuntimeError {
    let body = body.trim();
    let mut reason: String = body.chars().take(ERROR_CHARS).collect();
    if reason.len() < body.len() {
        reason.push('…');
    }
    if reason.is_empty() {
        RuntimeError::engine(format!("HTTP {status} with an empty body"))
    } else {
        RuntimeError::engine(format!("HTTP {status}: {reason}"))
    }
}

/// A runtime archive pinned by its SHA-256, and where its executable lands once unpacked.
pub(crate) struct Runtime {
    /// Names the runtime in errors.
    pub label: &'static str,
    pub lock_file: &'static str,
    pub install_dir: &'static str,
    /// The executable's path inside `install_dir`.
    pub executable: &'static str,
    pub sha256: &'static str,
}

impl Runtime {
    /// Unpacks `archive` (a model file in `dir`) once; later loads find the executable and return at once. A file lock
    /// keeps two loads from unpacking at the same time.
    pub(crate) fn install(&self, dir: &Path, archive: &str, stop: &AtomicBool) -> Result<PathBuf, RuntimeError> {
        let lock = std::fs::File::options()
            .create(true)
            .append(true)
            .open(dir.join(self.lock_file))
            .map_err(RuntimeError::engine)?;
        loop {
            cancelled(stop)?;
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(WATCH),
                Err(error) => return Err(RuntimeError::engine(error)),
            }
        }
        let destination = dir.join(self.install_dir);
        let binary = destination.join(self.executable);
        if binary.is_file() {
            return Ok(binary);
        }
        let label = self.label;
        let archive = super::model_file(dir, archive)?;
        let digest =
            block_on(async { dito_local::download::sha256_file(&archive).await.map_err(RuntimeError::engine) })?;
        if digest != self.sha256 {
            return Err(RuntimeError::engine(format!("{label} runtime archive checksum mismatch")));
        }
        cancelled(stop)?;
        let temporary = dir.join(format!("{}.installing", self.install_dir));
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
            return Err(RuntimeError::engine(format!("could not unpack the {label} runtime")));
        }
        if !temporary.join(self.executable).is_file() {
            return Err(RuntimeError::engine(format!("the {label} runtime package has no executable")));
        }
        if destination.exists() {
            std::fs::remove_dir_all(&destination).map_err(RuntimeError::engine)?;
        }
        std::fs::rename(&temporary, &destination).map_err(RuntimeError::engine)?;
        Ok(binary)
    }
}

pub(crate) struct Worker {
    process: ManagedProcess,
    endpoint: String,
    token: String,
}

impl Worker {
    /// Starts `binary` on a free loopback port behind a new random token, writing its output to `log`. `args` builds
    /// the command line from the port and the token.
    pub(crate) fn spawn(
        binary: &Path,
        log: &Path,
        args: impl FnOnce(u16, &str) -> Vec<String>,
    ) -> Result<Self, RuntimeError> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(RuntimeError::engine)?;
        let port = listener.local_addr().map_err(RuntimeError::engine)?.port();
        let mut secret = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut secret))
            .map_err(RuntimeError::engine)?;
        let token = STANDARD.encode(secret);
        let args = args(port, &token);
        drop(listener);
        let log = std::fs::File::create(log).map_err(RuntimeError::engine)?;
        let process = ManagedProcess::spawn(binary, &args, log)?;
        Ok(Self { process, endpoint: format!("http://127.0.0.1:{port}"), token })
    }

    /// Polls `path` until a successful answer whose JSON body (if any) `ready` accepts. Each probe runs to the
    /// client's own timeout: a server busy loading a large model may answer slowly, and dropping the request early
    /// would only send it again.
    pub(crate) fn wait_ready(
        &self,
        label: &str,
        log_name: &str,
        path: &str,
        stop: &AtomicBool,
        ready: impl Fn(Option<&Value>) -> bool,
    ) -> Result<(), RuntimeError> {
        block_on(async {
            let client = client(Duration::from_secs(2))?;
            let deadline = Instant::now() + STARTUP;
            loop {
                if Instant::now() >= deadline {
                    return Err(RuntimeError::engine(format!("{label} model startup timed out; see {log_name}")));
                }
                let probe = async {
                    let response = client
                        .get(format!("{}{path}", self.endpoint))
                        .bearer_auth(&self.token)
                        .send()
                        .await
                        .ok()
                        .filter(|response| response.status().is_success())?;
                    Some(response.json::<Value>().await.ok())
                };
                tokio::select! {
                    answer = probe => {
                        if let Some(body) = answer && ready(body.as_ref()) {
                            return Ok(());
                        }
                    }
                    error = self.interrupted(stop) => return Err(with_log(error, log_name)),
                }
                tokio::select! {
                    () = tokio::time::sleep(PROBE_GAP) => {}
                    error = self.interrupted(stop) => return Err(with_log(error, log_name)),
                }
            }
        })
    }

    /// A chat completion request, ready for its body.
    pub(crate) fn chat_request(&self, client: &reqwest::Client) -> reqwest::RequestBuilder {
        client.post(format!("{}/v1/chat/completions", self.endpoint)).bearer_auth(&self.token)
    }

    pub(crate) async fn post_chat(
        &self,
        client: &reqwest::Client,
        body: &Value,
        stop: &AtomicBool,
    ) -> Result<Value, RuntimeError> {
        let request = async {
            let response = self.chat_request(client).json(body).send().await.map_err(RuntimeError::engine)?;
            ensure_success(response).await?.json::<Value>().await.map_err(RuntimeError::engine)
        };
        tokio::select! {
            value = request => value,
            error = self.interrupted(stop) => Err(error),
        }
    }

    /// Resolves once the run is stopped or the process has failed, with the error that ends the request.
    pub(crate) async fn interrupted(&self, stop: &AtomicBool) -> RuntimeError {
        loop {
            if let Err(error) = cancelled(stop) {
                return error;
            }
            if let Some(error) = self.failure() {
                return RuntimeError::engine(error);
            }
            tokio::time::sleep(WATCH).await;
        }
    }

    pub(crate) fn failure(&self) -> Option<String> {
        self.process.failure()
    }

    pub(crate) fn shutdown(&self) {
        self.process.shutdown();
    }

    pub(crate) fn pid(&self) -> u32 {
        self.process.pid()
    }
}

fn with_log(error: RuntimeError, log_name: &str) -> RuntimeError {
    match error {
        RuntimeError::Cancelled => RuntimeError::Cancelled,
        error => RuntimeError::engine(format!("{error}; see {log_name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(status: u16, body: &str) -> String {
        match rejected(status, body) {
            RuntimeError::Engine(reason) => reason,
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn a_rejected_answer_keeps_its_status_and_the_servers_reason() {
        let body = r#"{"error":{"message":"the request exceeds the available context size, try increasing it","type":"exceed_context_size_error"}}"#;
        assert_eq!(reason(400, &format!(" \n{body}\n ")), format!("HTTP 400: {body}"));
        assert_eq!(reason(503, " \n"), "HTTP 503 with an empty body");
    }

    #[test]
    fn a_long_reason_is_cut_at_a_character_boundary_and_marked() {
        let long = "é".repeat(1_000);
        assert_eq!(reason(500, &long), format!("HTTP 500: {}…", "é".repeat(ERROR_CHARS)));
    }
}

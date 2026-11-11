//! Resumable single-file download with SHA-256 verification.

use std::path::Path;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum DlError {
    #[error("cancelled")]
    Cancelled,
    #[error("HTTP {0}")]
    Http(u16),
    #[error("network: {0}")]
    Network(String),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("size mismatch: expected {expected}, got {got}")]
    SizeMismatch { expected: u64, got: u64 },
    #[error("not enough free disk space")]
    InsufficientSpace,
    #[error("disk: {0}")]
    Io(String),
}

impl From<std::io::Error> for DlError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// `done` counts this file only.
#[derive(Debug, Clone, Copy)]
pub struct FileProgress {
    pub done: u64,
    pub bytes_per_sec: f64,
}

/// Verifying takes seconds for multi-GB files, so the UI shows it apart from transferring.
#[derive(Debug, Clone, Copy)]
pub enum Phase {
    Fetching(FileProgress),
    Verifying,
}

pub async fn sha256_file(path: &Path) -> std::io::Result<String> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex::encode(h.finalize()))
    })
    .await
    .map_err(std::io::Error::other)?
}

pub struct Target<'a> {
    pub url: &'a str,
    pub part: &'a Path,
    pub dest: &'a Path,
    pub size: u64,
    pub sha256: Option<&'a str>,
}

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// The checksum pass is cancel-aware: `Cancelled` keeps the completed `.part` so a later attempt skips straight to
/// verifying.
pub async fn fetch(
    client: &reqwest::Client,
    t: &Target<'_>,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DlError> {
    let have = resumable_len(t).await?;
    if have < t.size {
        let (response, file, start) = begin_transfer(client, t, have, cancel).await?;
        stream_body(response, file, start, cancel, on_progress).await?;
    }
    verify_and_install(t, cancel, on_progress).await
}

/// Bytes already in `.part`; a file larger than expected cannot be resumed and is discarded.
async fn resumable_len(t: &Target<'_>) -> Result<u64, DlError> {
    if let Some(dir) = t.part.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let have = tokio::fs::metadata(t.part).await.map(|m| m.len()).unwrap_or(0);
    if have > t.size {
        tokio::fs::remove_file(t.part).await?;
        return Ok(0);
    }
    Ok(have)
}

/// The returned offset is where the response body starts in the file.
async fn begin_transfer(
    client: &reqwest::Client,
    t: &Target<'_>,
    have: u64,
    cancel: &CancellationToken,
) -> Result<(reqwest::Response, tokio::fs::File, u64), DlError> {
    let mut request = client.get(t.url);
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(DlError::Cancelled),
        r = request.send() => r.map_err(|e| DlError::Network(e.to_string()))?,
    };
    let status = response.status().as_u16();
    if !matches!(status, 200 | 206) {
        return Err(DlError::Http(status));
    }
    // A server that ignores Range answers 200 with the whole file: start over.
    if have > 0 && status == 206 {
        let file = tokio::fs::OpenOptions::new().append(true).open(t.part).await?;
        return Ok((response, file, have));
    }
    Ok((response, tokio::fs::File::create(t.part).await?, 0))
}

async fn stream_body(
    response: reqwest::Response,
    mut file: tokio::fs::File,
    start: u64,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DlError> {
    let mut stream = response.bytes_stream();
    let started = Instant::now();
    let mut have = start;
    let mut last_report = started;
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                file.flush().await?;
                return Err(DlError::Cancelled);
            }
            n = stream.next() => n,
        };
        let Some(chunk) = next else { break };
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                file.flush().await?;
                return Err(DlError::Network(e.to_string()));
            }
        };
        file.write_all(&chunk).await?;
        have += chunk.len() as u64;
        if last_report.elapsed() >= PROGRESS_INTERVAL {
            last_report = Instant::now();
            on_progress(fetching(have, start, started));
        }
    }
    file.flush().await?;
    on_progress(fetching(have, start, started));
    Ok(())
}

fn fetching(have: u64, start: u64, started: Instant) -> Phase {
    let secs = started.elapsed().as_secs_f64().max(0.001);
    Phase::Fetching(FileProgress { done: have, bytes_per_sec: (have - start) as f64 / secs })
}

async fn verify_and_install(
    t: &Target<'_>,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DlError> {
    let got = tokio::fs::metadata(t.part).await?.len();
    if got != t.size {
        // Truncated by the server or connection: keep the bytes so the next attempt resumes.
        return Err(DlError::SizeMismatch { expected: t.size, got });
    }
    if let Some(want) = t.sha256 {
        on_progress(Phase::Verifying);
        let actual = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(DlError::Cancelled),
            h = sha256_file(t.part) => h?,
        };
        if !actual.eq_ignore_ascii_case(want) {
            let _ = tokio::fs::remove_file(t.part).await;
            return Err(DlError::ChecksumMismatch);
        }
    }
    tokio::fs::rename(t.part, t.dest).await?;
    Ok(())
}

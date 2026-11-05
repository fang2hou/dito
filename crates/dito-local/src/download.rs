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
    #[error("disk: {0}")]
    Io(String),
}

impl From<std::io::Error> for DlError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Bytes of *this file* written so far, and the instantaneous speed.
#[derive(Debug, Clone, Copy)]
pub struct FileProgress {
    pub done: u64,
    pub bytes_per_sec: f64,
}

/// Phase of a single-file fetch, reported through the progress callback so the UI can
/// tell "still transferring" from "verifying the checksum" (seconds for multi-GB files).
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

/// What to fetch and where it goes.
pub struct Target<'a> {
    pub url: &'a str,
    pub part: &'a Path,
    pub dest: &'a Path,
    pub size: u64,
    pub sha256: Option<&'a str>,
}

/// Download `t.url` into `t.part` (resuming if it already has bytes), verify, and rename to `t.dest`.
///
/// The checksum pass runs for several seconds on multi-GB model files; it is cancel-aware
/// (`Cancelled` keeps the completed `.part` so a later attempt skips straight to verifying)
/// and is announced through the callback as [`Phase::Verifying`].
pub async fn fetch(
    client: &reqwest::Client,
    t: &Target<'_>,
    cancel: &CancellationToken,
    on_progress: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DlError> {
    let Target { url, part, dest, size: expected_size, sha256 } = *t;
    if let Some(dir) = part.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut have = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
    if have > expected_size {
        tokio::fs::remove_file(part).await?;
        have = 0;
    }

    if have < expected_size {
        let mut req = client.get(url);
        if have > 0 {
            req = req.header("Range", format!("bytes={have}-"));
        }
        let resp = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(DlError::Cancelled),
            r = req.send() => r.map_err(|e| DlError::Network(e.to_string()))?,
        };
        let status = resp.status().as_u16();
        if !(status == 200 || status == 206) {
            return Err(DlError::Http(status));
        }
        // A server that ignores Range answers 200 with the whole file: start over.
        let append = have > 0 && status == 206;
        let mut file = if append {
            tokio::fs::OpenOptions::new().append(true).open(part).await?
        } else {
            have = 0;
            tokio::fs::File::create(part).await?
        };

        let mut stream = resp.bytes_stream();
        let started = Instant::now();
        let started_at = have;
        let mut last_report = Instant::now();
        loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    file.flush().await?;
                    return Err(DlError::Cancelled);
                }
                n = stream.next() => n,
            };
            match next {
                Some(Ok(chunk)) => {
                    file.write_all(&chunk).await?;
                    have += chunk.len() as u64;
                    if last_report.elapsed() >= Duration::from_millis(100) {
                        last_report = Instant::now();
                        let secs = started.elapsed().as_secs_f64().max(0.001);
                        on_progress(Phase::Fetching(FileProgress {
                            done: have,
                            bytes_per_sec: (have - started_at) as f64 / secs,
                        }));
                    }
                }
                Some(Err(e)) => {
                    file.flush().await?;
                    return Err(DlError::Network(e.to_string()));
                }
                None => break,
            }
        }
        file.flush().await?;
        drop(file);
        let secs = started.elapsed().as_secs_f64().max(0.001);
        on_progress(Phase::Fetching(FileProgress { done: have, bytes_per_sec: (have - started_at) as f64 / secs }));
    }

    let got = tokio::fs::metadata(part).await?.len();
    if got != expected_size {
        // Truncated by the server or connection: keep the bytes so the next attempt resumes.
        return Err(DlError::SizeMismatch { expected: expected_size, got });
    }
    if let Some(want) = sha256 {
        on_progress(Phase::Verifying);
        let actual = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(DlError::Cancelled),
            h = sha256_file(part) => h?,
        };
        if !actual.eq_ignore_ascii_case(want) {
            let _ = tokio::fs::remove_file(part).await;
            return Err(DlError::ChecksumMismatch);
        }
    }
    tokio::fs::rename(part, dest).await?;
    Ok(())
}

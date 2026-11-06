use std::time::Duration;

/// What the server does next. Mutate through [`MockServer::behavior`](crate::MockServer::behavior).
#[derive(Debug, Clone)]
pub struct Behavior {
    /// Expected secret: `api-key` header (Azure) or `Authorization: Bearer` (compat). `None` = no auth.
    pub expected_key: Option<String>,
    pub latency: Duration,
    /// Fail every request with this status (e.g. 429, 500).
    pub fail_status: Option<u16>,
    /// Reject `reasoning_effort` like a non-reasoning model would.
    pub reject_reasoning_effort: bool,
    pub transcript: String,
    pub language: Option<String>,
    /// File downloads: cut the connection after this many body bytes (first matching request only).
    pub cut_download_after: Option<usize>,
    /// File downloads: pause between 16 KiB chunks.
    pub download_chunk_delay: Duration,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            expected_key: None,
            latency: Duration::ZERO,
            fail_status: None,
            reject_reasoning_effort: false,
            transcript: "えーと 明日の3時に会議があります".into(),
            language: Some("ja".into()),
            cut_download_after: None,
            download_chunk_delay: Duration::ZERO,
        }
    }
}

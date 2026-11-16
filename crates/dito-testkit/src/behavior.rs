use std::time::Duration;

/// Mutate through [`MockServer::behavior`](crate::MockServer::behavior).
#[derive(Debug, Clone)]
pub struct Behavior {
    /// Expected secret: `api-key` header (Azure) or `Authorization: Bearer` (compat). `None` = no auth.
    pub expected_key: Option<String>,
    pub latency: Duration,
    pub fail_status: Option<u16>,
    pub reject_reasoning_effort: bool,
    /// Chat completions end with `finish_reason: "length"`.
    pub truncate_chat: bool,
    /// Streamed chat completions: pause between content events.
    pub chat_chunk_delay: Duration,
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
            truncate_chat: false,
            chat_chunk_delay: Duration::from_millis(40),
            transcript: "えーと 明日の3時に会議があります".into(),
            language: Some("ja".into()),
            cut_download_after: None,
            download_chunk_delay: Duration::ZERO,
        }
    }
}

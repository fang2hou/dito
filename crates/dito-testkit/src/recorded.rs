use axum::http::HeaderMap;

#[derive(Debug, Clone, Default)]
pub struct Recorded {
    pub path: String,
    pub model: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub audio_bytes: usize,
    pub system: Option<String>,
    pub user: Option<String>,
    pub reasoning_effort: Option<String>,
    pub headers: Vec<(String, String)>,
}

pub(crate) fn header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers.iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string())).collect()
}

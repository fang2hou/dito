use serde::{Deserialize, Serialize};

/// User-facing failure class. Drives the HUD error variants and the short message in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    /// 401/403, missing or rejected key / credentials.
    Auth,
    /// Network unreachable, DNS, TLS, connection reset.
    Network,
    Timeout,
    RateLimit,
    /// Deployment / model id does not exist at the endpoint.
    ModelNotFound,
    /// Response could not be understood.
    BadResponse,
    /// Audio was empty or contained no speech.
    NoSpeech,
    /// Microphone unavailable or permission denied.
    Mic,
    /// Engine (local model) is not running / not installed.
    Unavailable,
    Other,
}

impl ErrorKind {
    const ALL: [ErrorKind; 10] = [
        Self::Auth,
        Self::Network,
        Self::Timeout,
        Self::RateLimit,
        Self::ModelNotFound,
        Self::BadResponse,
        Self::NoSpeech,
        Self::Mic,
        Self::Unavailable,
        Self::Other,
    ];

    /// Split a stored `Kind: detail` error line (the `Display` of [`ProviderError`]) back into its parts, so the
    /// UI can show the kind in the user's language and keep the technical detail as is.
    pub fn decode(line: &str) -> Option<(ErrorKind, &str)> {
        let (head, detail) = line.split_once(": ")?;
        Self::ALL.into_iter().find(|k| format!("{k:?}") == head).map(|k| (k, detail))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub fn from_status(status: u16, body: &str) -> Self {
        let kind = match status {
            401 | 403 => ErrorKind::Auth,
            404 => ErrorKind::ModelNotFound,
            408 | 504 => ErrorKind::Timeout,
            429 => ErrorKind::RateLimit,
            _ => ErrorKind::Other,
        };
        let snippet: String = body.chars().take(200).collect();
        Self::new(kind, format!("HTTP {status}: {snippet}"))
    }

    pub fn from_reqwest(err: &reqwest::Error) -> Self {
        let kind = if err.is_timeout() {
            ErrorKind::Timeout
        } else if err.is_connect() || err.is_request() {
            ErrorKind::Network
        } else if err.is_decode() {
            ErrorKind::BadResponse
        } else {
            ErrorKind::Other
        };
        Self::new(kind, err.to_string())
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_error_lines_decode_to_kind_and_detail() {
        let e = ProviderError::new(ErrorKind::RateLimit, "HTTP 429: slow down");
        assert_eq!(ErrorKind::decode(&e.to_string()), Some((ErrorKind::RateLimit, "HTTP 429: slow down")));
        assert_eq!(ErrorKind::decode("plain text"), None);
        assert_eq!(ErrorKind::decode("Nope: x"), None);
    }
}

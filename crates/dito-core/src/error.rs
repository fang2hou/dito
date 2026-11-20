use serde::{Deserialize, Serialize};

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
    BadResponse,
    NoSpeech,
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

    /// The name of this kind in a stored error line. Written out by hand because history rows keep these strings:
    /// renaming a variant must not change what is stored.
    pub fn code(self) -> &'static str {
        match self {
            Self::Auth => "Auth",
            Self::Network => "Network",
            Self::Timeout => "Timeout",
            Self::RateLimit => "RateLimit",
            Self::ModelNotFound => "ModelNotFound",
            Self::BadResponse => "BadResponse",
            Self::NoSpeech => "NoSpeech",
            Self::Mic => "Mic",
            Self::Unavailable => "Unavailable",
            Self::Other => "Other",
        }
    }

    /// Splits a stored error line (the `Display` of [`ProviderError`]) so the UI can show the kind in the user's
    /// language and keep the technical detail as is.
    pub fn decode(line: &str) -> Option<(ErrorKind, &str)> {
        let (head, detail) = line.split_once(": ")?;
        Self::ALL.into_iter().find(|k| k.code() == head).map(|k| (k, detail))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {message}", .kind.code())]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), retryable: false }
    }

    pub fn retryable(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), retryable: true }
    }

    pub fn is_retryable(&self) -> bool {
        self.retryable
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
        let (kind, retryable) = classify_reqwest(err.is_timeout(), err.is_connect(), err.is_request(), err.is_decode());
        let message = err.to_string();
        if retryable { Self::retryable(kind, message) } else { Self::new(kind, message) }
    }
}

fn classify_reqwest(is_timeout: bool, is_connect: bool, is_request: bool, is_decode: bool) -> (ErrorKind, bool) {
    let kind = if is_timeout {
        ErrorKind::Timeout
    } else if is_connect || is_request {
        ErrorKind::Network
    } else if is_decode {
        ErrorKind::BadResponse
    } else {
        ErrorKind::Other
    };
    (kind, is_connect)
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

    #[test]
    fn retry_safety_does_not_change_stored_error_lines() {
        let ordinary = ProviderError::new(ErrorKind::Timeout, "waiting");
        let retryable = ProviderError::retryable(ErrorKind::Timeout, "waiting");
        assert!(!ordinary.is_retryable());
        assert!(retryable.is_retryable());
        assert_eq!(ordinary.to_string(), retryable.to_string());
        assert_eq!(ErrorKind::decode(&retryable.to_string()), Some((ErrorKind::Timeout, "waiting")));
    }

    #[test]
    fn only_reqwest_connection_failures_are_retryable() {
        assert_eq!(classify_reqwest(false, true, true, false), (ErrorKind::Network, true));
        assert_eq!(classify_reqwest(true, true, true, false), (ErrorKind::Timeout, true));
        assert_eq!(classify_reqwest(true, false, true, false), (ErrorKind::Timeout, false));
        assert_eq!(classify_reqwest(false, false, true, false), (ErrorKind::Network, false));
        assert_eq!(classify_reqwest(false, false, false, true), (ErrorKind::BadResponse, false));
        assert_eq!(classify_reqwest(false, false, false, false), (ErrorKind::Other, false));
    }

    #[test]
    fn codes_are_the_stored_format_of_every_kind() {
        // History rows store these strings: a variant may be renamed, its code may not.
        let pinned = [
            (ErrorKind::Auth, "Auth"),
            (ErrorKind::Network, "Network"),
            (ErrorKind::Timeout, "Timeout"),
            (ErrorKind::RateLimit, "RateLimit"),
            (ErrorKind::ModelNotFound, "ModelNotFound"),
            (ErrorKind::BadResponse, "BadResponse"),
            (ErrorKind::NoSpeech, "NoSpeech"),
            (ErrorKind::Mic, "Mic"),
            (ErrorKind::Unavailable, "Unavailable"),
            (ErrorKind::Other, "Other"),
        ];
        assert_eq!(pinned.len(), ErrorKind::ALL.len(), "every kind needs a pinned code");
        for (kind, code) in pinned {
            assert_eq!(kind.code(), code);
            let line = ProviderError::new(kind, "detail").to_string();
            assert_eq!(line, format!("{code}: detail"));
            assert_eq!(ErrorKind::decode(&line), Some((kind, "detail")));
        }
    }
}

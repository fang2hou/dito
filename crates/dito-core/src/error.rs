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

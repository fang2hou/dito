//! Who may talk to the service: loopback hosts only, no browser origins, and the shared access token.

use axum::http::{HeaderMap, header};

/// A request must name the loopback address and the port the service listens on. Anything else is a page on another
/// site that pointed its own name at this Mac (DNS rebinding).
pub(super) fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let Some((name, number)) = host.rsplit_once(':') else { return false };
    matches!(name, "127.0.0.1" | "localhost" | "[::1]") && number.parse::<u16>() == Ok(port)
}

/// Only a browser sends `Origin` with a request. The service has no page of its own, so every such request is refused,
/// whatever it names.
pub(super) fn has_browser_origin(headers: &HeaderMap) -> bool {
    headers.contains_key(header::ORIGIN)
}

/// The token of an `Authorization: Bearer` header.
pub(super) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(token.trim())
}

/// Compares without stopping at the first difference, so the time taken does not tell a client how much of a guess is
/// right. An empty token never matches.
pub(super) fn token_matches(expected: &str, presented: &str) -> bool {
    let (a, b) = (expected.as_bytes(), presented.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    std::hint::black_box(diff) == 0 && !a.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_names_on_the_listening_port_pass_the_host_check() {
        for host in ["127.0.0.1:17841", "localhost:17841", "[::1]:17841"] {
            assert!(host_allowed(Some(host), 17841), "{host}");
        }
        for host in
            ["evil.example:17841", "127.0.0.1.evil.example:17841", "127.0.0.1", "127.0.0.1:80", "0.0.0.0:17841", ""]
        {
            assert!(!host_allowed(Some(host), 17841), "{host}");
        }
        assert!(!host_allowed(None, 17841));
    }

    #[test]
    fn any_origin_header_marks_a_browser_request() {
        let mut headers = HeaderMap::new();
        assert!(!has_browser_origin(&headers));
        headers.insert(header::ORIGIN, "http://127.0.0.1:17841".parse().unwrap());
        assert!(has_browser_origin(&headers));
    }

    #[test]
    fn the_bearer_scheme_is_read_without_regard_to_case() {
        let get = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::AUTHORIZATION, value.parse().unwrap());
            bearer(&headers).map(str::to_owned)
        };
        assert_eq!(get("Bearer abc").as_deref(), Some("abc"));
        assert_eq!(get("bearer abc").as_deref(), Some("abc"));
        assert_eq!(get("Basic abc"), None);
        assert_eq!(get("Bearer"), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }

    #[test]
    fn tokens_match_only_when_equal_and_never_when_empty() {
        assert!(token_matches("dito_abc", "dito_abc"));
        assert!(!token_matches("dito_abc", "dito_abd"));
        assert!(!token_matches("dito_abc", "dito_ab"));
        assert!(!token_matches("dito_abc", "dito_abcd"));
        assert!(!token_matches("", ""));
        assert!(!token_matches("dito_abc", ""));
    }
}

use std::fmt;

use reqwest::Url;
use reqwest::header::HeaderName;
use serde_json::{Map, Value};

use crate::error::{ErrorKind, ProviderError};

const VARIABLES: [&str; 6] = ["model", "language", "api_key", "audio.base64", "audio.data_uri", "audio.file"];

const ALLOWED_KEYS: [(&str, &[&str]); 4] = [
    ("", &["version", "connection", "language_map", "request", "response"]),
    ("/connection", &["base_url", "model"]),
    ("/request", &["method", "path", "query", "headers", "encoding", "body", "fields"]),
    ("/response", &["text_pointer", "language_pointer"]),
];

/// A profile that breaks the connection config contract, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Path of the offending value: a JSON Pointer such as `/connection/model` or a dotted path such as
    /// `request.path`. Empty when the rule concerns no single field.
    pub field: String,
    pub reason: ValidationReason,
}

/// What is wrong with a profile. The `Display` text is English technical detail for logs and [`ProviderError`]; the
/// UI words each reason in the user's language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationReason {
    /// A required string is missing, blank or not a string.
    #[error("a value is required")]
    Required,
    /// `version` is not 1.
    #[error("must be 1")]
    UnsupportedVersion,
    /// `connection.base_url` does not parse as a URL.
    #[error("not a valid URL")]
    InvalidUrl,
    /// `connection.base_url` is not a plain HTTP(S) URL: another scheme, no host, a query, a fragment or credentials.
    #[error("must be an http or https URL without query, fragment or credentials")]
    NotHttpUrl,
    /// A section that must be a JSON object is not.
    #[error("must be an object")]
    ObjectRequired,
    /// `language_map` has an invalid or duplicate key, or a blank value.
    #[error("invalid language code or value")]
    InvalidLanguageMap,
    /// `request.method` is neither POST nor PUT.
    #[error("must be POST or PUT")]
    InvalidMethod,
    /// `request.path` is not a relative path starting with `/`.
    #[error("must be a relative path starting with /")]
    PathNotRelative,
    /// `request.encoding` does not match `body` and `fields`.
    #[error("encoding does not match body and fields")]
    EncodingMismatch,
    /// A response pointer is not a JSON Pointer.
    #[error("not a valid JSON Pointer")]
    InvalidJsonPointer,
    /// A key the contract does not define.
    #[error("unknown key")]
    UnknownKey,
    /// A header has an invalid name, or a value that is not a single-line string.
    #[error("invalid header name or value")]
    InvalidHeaders,
    /// A `query` or `fields` value is an array or an object.
    #[error("values must be strings, numbers, booleans or null")]
    NonScalarValue,
    /// `{{audio.file}}` outside `fields` in a multipart request.
    #[error("audio.file is only allowed in fields")]
    AudioFileOutsideFields,
    /// `{{language}}` as an array element.
    #[error("language cannot be an array element")]
    LanguageInArray,
    /// A `{{` without its `}}`.
    #[error("variable is missing its closing braces")]
    UnclosedVariable,
    /// A variable the contract does not define.
    #[error("unknown variable: {name}")]
    UnknownVariable { name: String },
    /// `{{audio.file}}` that is not the whole value of a multipart field.
    #[error("audio.file must be the whole value of a multipart field")]
    AudioFileNotWholeValue,
    /// `{{language}}` inside a longer string.
    #[error("language must be the whole value")]
    LanguageNotWholeValue,
}

impl ValidationError {
    fn new(field: impl Into<String>, reason: ValidationReason) -> Self {
        Self { field: field.into(), reason }
    }

    fn unlocated(reason: ValidationReason) -> Self {
        Self::new("", reason)
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.field.is_empty() { write!(f, "{}", self.reason) } else { write!(f, "{}: {}", self.field, self.reason) }
    }
}

impl std::error::Error for ValidationError {}

impl From<ValidationError> for ProviderError {
    fn from(error: ValidationError) -> Self {
        invalid(error.to_string())
    }
}

pub(super) fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorKind::Unavailable, message)
}

pub(super) fn string<'a>(v: &'a Value, path: &str) -> Result<&'a str, ValidationError> {
    v.pointer(path)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ValidationError::new(path, ValidationReason::Required))
}

pub(super) fn is_relative_path(path: &str) -> bool {
    path.starts_with('/') && !path.starts_with("//") && !path.contains(['?', '#'])
}

fn is_json_pointer(s: &str) -> bool {
    s.starts_with('/') && !s.ends_with('~') && !s.as_bytes().windows(2).any(|x| x[0] == b'~' && !b"01".contains(&x[1]))
}

/// Checks a profile against the connection config contract (`design/asr-json-config.md`).
pub fn validate(v: &Value) -> Result<(), ValidationError> {
    if v["version"] != 1 {
        return Err(ValidationError::new("version", ValidationReason::UnsupportedVersion));
    }
    check_connection(v)?;
    check_language_map(v)?;
    let encoding = check_request_head(v)?;
    check_request_sections(v)?;
    check_text_pointer(v)?;
    check_known_keys(v)?;
    check_headers(v)?;
    check_scalar_values(v)?;
    check_language_pointer(v)?;
    if encoding == "multipart" && audio_file_outside_fields(v) {
        return Err(ValidationError::unlocated(ValidationReason::AudioFileOutsideFields));
    }
    check_variables(&v["request"], encoding)
}

fn check_connection(v: &Value) -> Result<(), ValidationError> {
    let url = Url::parse(string(v, "/connection/base_url")?)
        .map_err(|_| ValidationError::new("connection.base_url", ValidationReason::InvalidUrl))?;
    let plain_http_url = ["http", "https"].contains(&url.scheme())
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if !plain_http_url {
        return Err(ValidationError::new("connection.base_url", ValidationReason::NotHttpUrl));
    }
    string(v, "/connection/model")?;
    Ok(())
}

fn is_language_tag(key: &str) -> bool {
    let parts = key.split('-').collect::<Vec<_>>();
    (2..=3).contains(&parts[0].len())
        && parts[0].bytes().all(|b| b.is_ascii_alphabetic())
        && parts[1..].iter().all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn check_language_map(v: &Value) -> Result<(), ValidationError> {
    string(v, "/language_map/other")?;
    let map = v["language_map"]
        .as_object()
        .ok_or_else(|| ValidationError::new("language_map", ValidationReason::ObjectRequired))?;
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in map {
        let valid_key = key == "other" || key == "auto" || is_language_tag(key);
        let valid_value = value.is_null() || value.as_str().is_some_and(|s| !s.trim().is_empty());
        if !valid_key || !seen.insert(key.to_ascii_lowercase()) || !valid_value {
            return Err(ValidationError::new("language_map", ValidationReason::InvalidLanguageMap));
        }
    }
    Ok(())
}

/// Validates method, path and the body shape, and returns the request encoding.
fn check_request_head(v: &Value) -> Result<&str, ValidationError> {
    if !["POST", "PUT"].contains(&string(v, "/request/method")?) {
        return Err(ValidationError::new("request.method", ValidationReason::InvalidMethod));
    }
    if !is_relative_path(string(v, "/request/path")?) {
        return Err(ValidationError::new("request.path", ValidationReason::PathNotRelative));
    }
    let request = &v["request"];
    let encoding = string(v, "/request/encoding")?;
    match encoding {
        "json" if request["body"].is_object() && request.get("fields").is_none() => {}
        "multipart" if request["fields"].is_object() && request.get("body").is_none() => {}
        _ => return Err(ValidationError::new("request", ValidationReason::EncodingMismatch)),
    }
    Ok(encoding)
}

fn request_object<'a>(v: &'a Value, field: &str) -> Result<&'a Map<String, Value>, ValidationError> {
    v["request"][field]
        .as_object()
        .ok_or_else(|| ValidationError::new(format!("request.{field}"), ValidationReason::ObjectRequired))
}

fn check_request_sections(v: &Value) -> Result<(), ValidationError> {
    if v["request"].get("query").is_some() {
        request_object(v, "query")?;
    }
    request_object(v, "headers")?;
    Ok(())
}

fn check_text_pointer(v: &Value) -> Result<(), ValidationError> {
    if is_json_pointer(string(v, "/response/text_pointer")?) {
        Ok(())
    } else {
        Err(ValidationError::new("response.text_pointer", ValidationReason::InvalidJsonPointer))
    }
}

fn check_known_keys(v: &Value) -> Result<(), ValidationError> {
    for (path, allowed) in ALLOWED_KEYS {
        let object = v
            .pointer(path)
            .and_then(Value::as_object)
            .ok_or_else(|| ValidationError::new(path, ValidationReason::ObjectRequired))?;
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(ValidationError::new(format!("{path}/{key}"), ValidationReason::UnknownKey));
        }
    }
    Ok(())
}

fn check_headers(v: &Value) -> Result<(), ValidationError> {
    for (name, value) in request_object(v, "headers")? {
        let valid = value.as_str().is_some_and(|s| !s.contains(['\r', '\n']))
            && HeaderName::from_bytes(name.as_bytes()).is_ok();
        if !valid {
            return Err(ValidationError::new("request.headers", ValidationReason::InvalidHeaders));
        }
    }
    Ok(())
}

fn check_scalar_values(v: &Value) -> Result<(), ValidationError> {
    for field in ["query", "fields"] {
        if let Some(object) = v["request"][field].as_object()
            && object.values().any(|v| v.is_array() || v.is_object())
        {
            return Err(ValidationError::new(format!("request.{field}"), ValidationReason::NonScalarValue));
        }
    }
    Ok(())
}

fn check_language_pointer(v: &Value) -> Result<(), ValidationError> {
    match v["response"].get("language_pointer") {
        Some(pointer) if !pointer.as_str().is_some_and(is_json_pointer) => {
            Err(ValidationError::new("response.language_pointer", ValidationReason::InvalidJsonPointer))
        }
        _ => Ok(()),
    }
}

fn audio_file_outside_fields(v: &Value) -> bool {
    ["headers", "query"].iter().any(|field| v["request"][field].to_string().contains("{{audio.file}}"))
}

fn check_variables(v: &Value, encoding: &str) -> Result<(), ValidationError> {
    match v {
        Value::String(s) => check_placeholders(s, encoding),
        Value::Array(items) => {
            for item in items {
                if item == "{{language}}" {
                    return Err(ValidationError::unlocated(ValidationReason::LanguageInArray));
                }
                check_variables(item, encoding)?;
            }
            Ok(())
        }
        Value::Object(object) => object.values().try_for_each(|item| check_variables(item, encoding)),
        _ => Ok(()),
    }
}

fn check_placeholders(s: &str, encoding: &str) -> Result<(), ValidationError> {
    let mut rest = s;
    while let Some((_, tail)) = rest.split_once("{{") {
        let (name, next) =
            tail.split_once("}}").ok_or_else(|| ValidationError::unlocated(ValidationReason::UnclosedVariable))?;
        if !VARIABLES.contains(&name) {
            return Err(ValidationError::unlocated(ValidationReason::UnknownVariable { name: name.to_owned() }));
        }
        if name == "audio.file" && (encoding != "multipart" || s != "{{audio.file}}") {
            return Err(ValidationError::unlocated(ValidationReason::AudioFileNotWholeValue));
        }
        if name == "language" && s != "{{language}}" {
            return Err(ValidationError::unlocated(ValidationReason::LanguageNotWholeValue));
        }
        rest = next;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::configured_asr::AsrProfiles;

    #[test]
    fn templates_require_model() {
        for profile in AsrProfiles::default().profiles {
            let mut v = profile.config;
            assert!(validate(&v).is_err());
            v["connection"]["model"] = "user-model".into();
            assert!(validate(&v).is_ok());
            v["language_map"].as_object_mut().unwrap().remove("other");
            assert!(validate(&v).is_err());
        }
    }

    #[test]
    fn failures_name_their_field_and_reason() {
        let template = include_str!("../../../../../design/asr-templates/funasr.json");
        let mut v: Value = serde_json::from_str(template).expect("bundled ASR template");
        assert_eq!(validate(&v), Err(ValidationError::new("/connection/model", ValidationReason::Required)));

        v["connection"]["model"] = "user-model".into();
        v["request"]["path"] = "services".into();
        assert_eq!(validate(&v), Err(ValidationError::new("request.path", ValidationReason::PathNotRelative)));

        v["request"]["path"] = "/services".into();
        v["request"]["headers"]["X-Trace"] = "{{lang}}".into();
        assert_eq!(
            validate(&v),
            Err(ValidationError::unlocated(ValidationReason::UnknownVariable { name: "lang".into() }))
        );
    }
}

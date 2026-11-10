use reqwest::Url;
use reqwest::header::HeaderName;
use serde_json::{Map, Value};

use crate::error::{ErrorKind, ProviderError, ProviderResult};

const VARIABLES: [&str; 6] = ["model", "language", "api_key", "audio.base64", "audio.data_uri", "audio.file"];

const ALLOWED_KEYS: [(&str, &[&str]); 4] = [
    ("", &["version", "connection", "language_map", "request", "response"]),
    ("/connection", &["base_url", "model"]),
    ("/request", &["method", "path", "query", "headers", "encoding", "body", "fields"]),
    ("/response", &["text_pointer", "language_pointer"]),
];

pub(super) fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorKind::Unavailable, message)
}

pub(super) fn string<'a>(v: &'a Value, path: &str) -> ProviderResult<&'a str> {
    v.pointer(path)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(format!("{path}: 値を入力してください。")))
}

pub(super) fn is_relative_path(path: &str) -> bool {
    path.starts_with('/') && !path.starts_with("//") && !path.contains(['?', '#'])
}

fn is_json_pointer(s: &str) -> bool {
    s.starts_with('/') && !s.ends_with('~') && !s.as_bytes().windows(2).any(|x| x[0] == b'~' && !b"01".contains(&x[1]))
}

/// Checks a profile against the connection config contract (`design/asr-json-config.md`).
pub fn validate(v: &Value) -> ProviderResult<()> {
    if v["version"] != 1 {
        return Err(invalid("version: 1 を指定してください。"));
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
        return Err(invalid("audio.file は fields 内に指定してください。"));
    }
    check_variables(&v["request"], encoding)
}

fn check_connection(v: &Value) -> ProviderResult<()> {
    let url = Url::parse(string(v, "/connection/base_url")?)
        .map_err(|_| invalid("connection.base_url: URL を確認してください。"))?;
    let plain_http_url = ["http", "https"].contains(&url.scheme())
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if !plain_http_url {
        return Err(invalid("connection.base_url: HTTP URL を指定してください。"));
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

fn check_language_map(v: &Value) -> ProviderResult<()> {
    string(v, "/language_map/other")?;
    let map = v["language_map"].as_object().ok_or_else(|| invalid("language_map: オブジェクトを指定してください。"))?;
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in map {
        let valid_key = key == "other" || key == "auto" || is_language_tag(key);
        let valid_value = value.is_null() || value.as_str().is_some_and(|s| !s.trim().is_empty());
        if !valid_key || !seen.insert(key.to_ascii_lowercase()) || !valid_value {
            return Err(invalid("language_map: 言語コードと値を確認してください。"));
        }
    }
    Ok(())
}

/// Validates method, path and the body shape, and returns the request encoding.
fn check_request_head(v: &Value) -> ProviderResult<&str> {
    if !["POST", "PUT"].contains(&string(v, "/request/method")?) {
        return Err(invalid("request.method: POST または PUT を指定してください。"));
    }
    if !is_relative_path(string(v, "/request/path")?) {
        return Err(invalid("request.path: 相対パスを指定してください。"));
    }
    let request = &v["request"];
    let encoding = string(v, "/request/encoding")?;
    match encoding {
        "json" if request["body"].is_object() && request.get("fields").is_none() => {}
        "multipart" if request["fields"].is_object() && request.get("body").is_none() => {}
        _ => return Err(invalid("request: encoding と body / fields を確認してください。")),
    }
    Ok(encoding)
}

fn request_object<'a>(v: &'a Value, field: &str) -> ProviderResult<&'a Map<String, Value>> {
    v["request"][field].as_object().ok_or_else(|| invalid(format!("request.{field}: オブジェクトを指定してください。")))
}

fn check_request_sections(v: &Value) -> ProviderResult<()> {
    if v["request"].get("query").is_some() {
        request_object(v, "query")?;
    }
    request_object(v, "headers")?;
    Ok(())
}

fn check_text_pointer(v: &Value) -> ProviderResult<()> {
    if is_json_pointer(string(v, "/response/text_pointer")?) {
        Ok(())
    } else {
        Err(invalid("response.text_pointer: JSON Pointer を確認してください。"))
    }
}

fn check_known_keys(v: &Value) -> ProviderResult<()> {
    for (path, allowed) in ALLOWED_KEYS {
        let object = v
            .pointer(path)
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(format!("{path}: オブジェクトを指定してください。")))?;
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(invalid(format!("{path}/{key}: 未定義の項目です。")));
        }
    }
    Ok(())
}

fn check_headers(v: &Value) -> ProviderResult<()> {
    for (name, value) in request_object(v, "headers")? {
        let valid = value.as_str().is_some_and(|s| !s.contains(['\r', '\n']))
            && HeaderName::from_bytes(name.as_bytes()).is_ok();
        if !valid {
            return Err(invalid("request.headers: ヘッダーを確認してください。"));
        }
    }
    Ok(())
}

fn check_scalar_values(v: &Value) -> ProviderResult<()> {
    for field in ["query", "fields"] {
        if let Some(object) = v["request"][field].as_object()
            && object.values().any(|v| v.is_array() || v.is_object())
        {
            return Err(invalid(format!("request.{field}: 値には文字列、数値、真偽値、null を指定してください。")));
        }
    }
    Ok(())
}

fn check_language_pointer(v: &Value) -> ProviderResult<()> {
    match v["response"].get("language_pointer") {
        Some(pointer) if !pointer.as_str().is_some_and(is_json_pointer) => {
            Err(invalid("response.language_pointer: JSON Pointer を確認してください。"))
        }
        _ => Ok(()),
    }
}

fn audio_file_outside_fields(v: &Value) -> bool {
    ["headers", "query"].iter().any(|field| v["request"][field].to_string().contains("{{audio.file}}"))
}

fn check_variables(v: &Value, encoding: &str) -> ProviderResult<()> {
    match v {
        Value::String(s) => check_placeholders(s, encoding),
        Value::Array(items) => {
            for item in items {
                if item == "{{language}}" {
                    return Err(invalid("language を配列の要素に指定できません。"));
                }
                check_variables(item, encoding)?;
            }
            Ok(())
        }
        Value::Object(object) => object.values().try_for_each(|item| check_variables(item, encoding)),
        _ => Ok(()),
    }
}

fn check_placeholders(s: &str, encoding: &str) -> ProviderResult<()> {
    let mut rest = s;
    while let Some((_, tail)) = rest.split_once("{{") {
        let (name, next) = tail.split_once("}}").ok_or_else(|| invalid("変数の閉じ括弧がありません。"))?;
        if !VARIABLES.contains(&name) {
            return Err(invalid(format!("未定義の変数: {name}")));
        }
        if name == "audio.file" && (encoding != "multipart" || s != "{{audio.file}}") {
            return Err(invalid("audio.file は multipart の完全な値に指定してください。"));
        }
        if name == "language" && s != "{{language}}" {
            return Err(invalid("language は完全な値に指定してください。"));
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
}

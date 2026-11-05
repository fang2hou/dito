//! User-configurable synchronous ASR requests. Secrets are supplied at runtime only.
use super::{AsrRequest, Transcriber, Transcript, default_timeout};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrProfiles {
    pub selected: usize,
    #[serde(with = "profile_json")]
    pub profiles: Vec<Value>,
}
impl Default for AsrProfiles {
    fn default() -> Self {
        Self {
            selected: 0,
            profiles: [
                include_str!("../../../../design/asr-templates/funasr.json"),
                include_str!("../../../../design/asr-templates/qwen-asr.json"),
                include_str!("../../../../design/asr-templates/openrouter.json"),
            ]
            .iter()
            .map(|s| serde_json::from_str(s).expect("bundled ASR template"))
            .collect(),
        }
    }
}
impl AsrProfiles {
    pub const NAMES: [&'static str; 3] = ["FunASR", "Qwen ASR", "OpenRouter"];
    pub fn active(&self) -> &Value {
        self.profiles.get(self.selected).unwrap_or(&Value::Null)
    }
    pub fn account(&self) -> String {
        format!("asr.{}", ["funasr", "qwen_asr", "openrouter"].get(self.selected).unwrap_or(&"funasr"))
    }
}
fn invalid(s: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorKind::Unavailable, s)
}
fn string<'a>(v: &'a Value, path: &str) -> ProviderResult<&'a str> {
    v.pointer(path)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(format!("{path}: 値を入力してください。")))
}
fn valid_pointer(s: &str) -> bool {
    s.starts_with('/') && !s.ends_with('~') && !s.as_bytes().windows(2).any(|x| x[0] == b'~' && !b"01".contains(&x[1]))
}
pub fn validate(v: &Value) -> ProviderResult<()> {
    if v["version"] != 1 {
        return Err(invalid("version: 1 を指定してください。"));
    }
    let url = reqwest::Url::parse(string(v, "/connection/base_url")?)
        .map_err(|_| invalid("connection.base_url: URL を確認してください。"))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("connection.base_url: HTTP URL を指定してください。"));
    }
    string(v, "/connection/model")?;
    string(v, "/language_map/other")?;
    let map = v["language_map"].as_object().ok_or_else(|| invalid("language_map: オブジェクトを指定してください。"))?;
    let mut keys = std::collections::BTreeSet::new();
    for (k, value) in map {
        let parts = k.split('-').collect::<Vec<_>>();
        let tag = (2..=3).contains(&parts[0].len())
            && parts[0].bytes().all(|b| b.is_ascii_alphabetic())
            && parts[1..].iter().all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()));
        if (k != "other" && k != "auto" && !tag)
            || !keys.insert(k.to_ascii_lowercase())
            || !(value.is_null() || value.as_str().is_some_and(|s| !s.trim().is_empty()))
        {
            return Err(invalid("language_map: 言語コードと値を確認してください。"));
        }
    }
    if !["POST", "PUT"].contains(&string(v, "/request/method")?) {
        return Err(invalid("request.method: POST または PUT を指定してください。"));
    }
    let path = string(v, "/request/path")?;
    if !path.starts_with('/') || path.starts_with("//") || path.contains(['?', '#']) {
        return Err(invalid("request.path: 相対パスを指定してください。"));
    }
    let encoding = string(v, "/request/encoding")?;
    match encoding {
        "json" if v["request"]["body"].is_object() && v["request"].get("fields").is_none() => {}
        "multipart" if v["request"]["fields"].is_object() && v["request"].get("body").is_none() => {}
        _ => return Err(invalid("request: encoding と body / fields を確認してください。")),
    }
    for field in ["query", "headers"] {
        if field == "query" && v["request"].get(field).is_none() {
            continue;
        }
        if !v["request"][field].is_object() {
            return Err(invalid(format!("request.{field}: オブジェクトを指定してください。")));
        }
    }
    let pointer = string(v, "/response/text_pointer")?;
    if !pointer.starts_with('/')
        || pointer.as_bytes().windows(2).any(|x| x[0] == b'~' && !b"01".contains(&x[1]))
        || pointer.ends_with('~')
    {
        return Err(invalid("response.text_pointer: JSON Pointer を確認してください。"));
    }
    for (path, allowed) in [
        ("", &["version", "connection", "language_map", "request", "response"][..]),
        ("/connection", &["base_url", "model"][..]),
        ("/request", &["method", "path", "query", "headers", "encoding", "body", "fields"][..]),
        ("/response", &["text_pointer", "language_pointer"][..]),
    ] {
        let object = v
            .pointer(path)
            .and_then(Value::as_object)
            .ok_or_else(|| invalid(format!("{path}: オブジェクトを指定してください。")))?;
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(invalid(format!("{path}/{key}: 未定義の項目です。")));
        }
    }
    for (name, value) in v["request"]["headers"].as_object().unwrap() {
        if value.as_str().is_none()
            || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
            || value.as_str().is_some_and(|s| s.contains(['\r', '\n']))
        {
            return Err(invalid("request.headers: ヘッダーを確認してください。"));
        }
    }
    for field in ["query", "fields"] {
        if let Some(object) = v["request"][field].as_object()
            && object.values().any(|v| v.is_array() || v.is_object())
        {
            return Err(invalid(format!("request.{field}: 値には文字列、数値、真偽値、null を指定してください。")));
        }
    }
    if let Some(pointer) = v["response"].get("language_pointer")
        && !pointer.as_str().is_some_and(valid_pointer)
    {
        return Err(invalid("response.language_pointer: JSON Pointer を確認してください。"));
    }
    fn vars(v: &Value, encoding: &str) -> ProviderResult<()> {
        match v {
            Value::String(s) => {
                let mut rest = s.as_str();
                while let Some((_, tail)) = rest.split_once("{{") {
                    let (name, next) = tail.split_once("}}").ok_or_else(|| invalid("変数の閉じ括弧がありません。"))?;
                    if !["model", "language", "api_key", "audio.base64", "audio.data_uri", "audio.file"].contains(&name)
                    {
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
            }
            Value::Array(a) => {
                for x in a {
                    if x == "{{language}}" {
                        return Err(invalid("language を配列の要素に指定できません。"));
                    }
                    vars(x, encoding)?;
                }
            }
            Value::Object(o) => {
                for x in o.values() {
                    vars(x, encoding)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    if encoding == "multipart"
        && (v["request"]["headers"].to_string().contains("{{audio.file}}")
            || v["request"]["query"].to_string().contains("{{audio.file}}"))
    {
        return Err(invalid("audio.file は fields 内に指定してください。"));
    }
    vars(&v["request"], encoding)
}
pub fn mapped_language(v: &Value, language: Option<&str>) -> Option<String> {
    let language = language.unwrap_or("auto").to_ascii_lowercase();
    let map = v["language_map"].as_object()?;
    let get = |key: &str| map.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v);
    let value = if language == "auto" {
        get("auto")
    } else {
        get(&language).or_else(|| get(language.split('-').next().unwrap_or(&language))).or_else(|| get("other"))
    };
    value.and_then(Value::as_str).map(str::to_owned)
}
fn expand(v: &Value, vars: &BTreeMap<&str, String>, language: Option<&str>) -> Option<Value> {
    Some(match v {
        Value::String(s) if s == "{{language}}" => Value::String(language?.to_owned()),
        Value::String(s) => {
            let mut output = String::new();
            let mut remaining = s.as_str();
            while let Some((prefix, tail)) = remaining.split_once("{{") {
                output.push_str(prefix);
                let Some((key, next)) = tail.split_once("}}") else {
                    output.push_str("{{");
                    output.push_str(tail);
                    remaining = "";
                    break;
                };
                if let Some(value) = vars.get(key) {
                    output.push_str(value);
                } else {
                    output.push_str("{{");
                    output.push_str(key);
                    output.push_str("}}");
                }
                remaining = next;
            }
            output.push_str(remaining);
            Value::String(output)
        }
        Value::Object(o) => {
            Value::Object(o.iter().filter_map(|(k, v)| Some((k.clone(), expand(v, vars, language)?))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().filter_map(|v| expand(v, vars, language)).collect()),
        _ => v.clone(),
    })
}
pub struct ConfiguredAsr {
    config: Value,
    key: String,
    client: reqwest::Client,
}
impl ConfiguredAsr {
    pub fn new(config: Value, key: String) -> ProviderResult<Self> {
        validate(&config)?;
        let client = reqwest::Client::builder()
            .timeout(default_timeout())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::from_reqwest(&e.without_url()))?;
        Ok(Self { config, key, client })
    }
}
#[async_trait]
impl Transcriber for ConfiguredAsr {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        use base64::Engine;
        let c = &self.config;
        let audio = base64::engine::general_purpose::STANDARD.encode(req.clip.to_wav());
        let vars = BTreeMap::from([
            ("model", string(c, "/connection/model")?.to_owned()),
            ("api_key", self.key.clone()),
            ("audio.base64", audio.clone()),
            ("audio.data_uri", format!("data:audio/wav;base64,{audio}")),
        ]);
        let language = mapped_language(c, req.language);
        let r = expand(&c["request"], &vars, language.as_deref()).ok_or_else(|| invalid("request"))?;
        let path = string(&r, "/path")?;
        if !path.starts_with('/') || path.starts_with("//") || path.contains(['?', '#']) {
            return Err(invalid("request.path: 相対パスを確認してください。"));
        }
        let url = format!("{}{}", string(c, "/connection/base_url")?.trim_end_matches('/'), string(&r, "/path")?);
        let method =
            reqwest::Method::from_bytes(string(c, "/request/method")?.as_bytes()).map_err(|_| invalid("method"))?;
        let mut url = reqwest::Url::parse(&url).map_err(|_| invalid("URL"))?;
        for (k, v) in r["query"].as_object().into_iter().flatten() {
            url.query_pairs_mut().append_pair(k, &v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()));
        }
        let mut request = self.client.request(method, url);
        for (k, v) in r["headers"].as_object().unwrap() {
            if k.eq_ignore_ascii_case("content-type") {
                continue;
            }
            request = request.header(k, v.as_str().ok_or_else(|| invalid("request.headers"))?);
        }
        if r["encoding"] == "multipart" {
            let mut form = reqwest::multipart::Form::new();
            for (k, v) in r["fields"].as_object().unwrap() {
                form = if v == "{{audio.file}}" {
                    form.part(
                        k.clone(),
                        reqwest::multipart::Part::bytes(req.clip.to_wav())
                            .file_name("audio.wav")
                            .mime_str("audio/wav")
                            .map_err(|e| ProviderError::from_reqwest(&e.without_url()))?,
                    )
                } else {
                    form.text(k.clone(), v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))
                };
            }
            request = request.multipart(form);
        } else {
            request = request.json(&r["body"]);
        }
        let response = request.send().await.map_err(|e| ProviderError::from_reqwest(&e.without_url()))?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status().as_u16(), "ASR request failed"));
        }
        let value: Value = response.json().await.map_err(|e| ProviderError::from_reqwest(&e.without_url()))?;
        let text = value
            .pointer(string(c, "/response/text_pointer")?)
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::new(ErrorKind::BadResponse, "認識結果を取得できません。"))?
            .trim()
            .to_owned();
        if text.is_empty() {
            return Err(ProviderError::new(ErrorKind::NoSpeech, "音声を認識できませんでした。"));
        }
        let language = c["response"]["language_pointer"]
            .as_str()
            .and_then(|p| value.pointer(p))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(Transcript { text, language })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_values_are_not_interpreted_as_more_placeholders() {
        let vars = BTreeMap::from([("api_key", "{{model}}".to_owned()), ("model", "actual-model".to_owned())]);
        assert_eq!(
            expand(&Value::String("Bearer {{api_key}}".into()), &vars, None),
            Some(Value::String("Bearer {{model}}".into()))
        );
    }
    #[test]
    fn language_fallback_and_auto() {
        let v = serde_json::json!({"language_map":{"ja":"JP","other":"EN","auto":null}});
        assert_eq!(mapped_language(&v, Some("ja-JP")), Some("JP".into()));
        assert_eq!(mapped_language(&v, Some("fr")), Some("EN".into()));
        assert_eq!(mapped_language(&v, None), None);
    }
    #[test]
    fn templates_require_model() {
        for mut v in AsrProfiles::default().profiles {
            assert!(validate(&v).is_err());
            v["connection"]["model"] = "user-model".into();
            assert!(validate(&v).is_ok());
            v["language_map"].as_object_mut().unwrap().remove("other");
            assert!(validate(&v).is_err());
        }
    }
    #[test]
    fn omit_language_without_altering_explicit_null() {
        let v = serde_json::json!({"language":"{{language}}","keep":null});
        assert_eq!(expand(&v, &BTreeMap::new(), None), Some(serde_json::json!({"keep":null})));
    }
}

mod profile_json {
    use super::*;
    pub fn serialize<S: serde::Serializer>(value: &[Value], serializer: S) -> Result<S::Ok, S::Error> {
        value.iter().map(Value::to_string).collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Value>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|s| serde_json::from_str(&s).map_err(serde::de::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn profile_nulls_and_custom_requests_survive_toml_storage() {
        let mut config = crate::config::Config::default();
        let mut profiles = AsrProfiles { selected: 2, ..Default::default() };
        profiles.profiles[2]["connection"]["model"] = "my-model".into();
        profiles.profiles[2]["language_map"]["ja"] = "JP".into();
        config.asr.cloud_asr = Some(profiles);
        let encoded = toml::to_string(&config).unwrap();
        let decoded: crate::config::Config = toml::from_str(&encoded).unwrap();
        assert_eq!(config, decoded);
        assert!(decoded.asr.cloud_asr.unwrap().active()["language_map"]["auto"].is_null());
    }
    #[test]
    fn reasoning_effort_accepts_future_values_and_empty_default() {
        for value in ["low", "", "xhigh", "provider-new-value"] {
            let effort: crate::config::ReasoningEffort = value.to_owned().into();
            assert_eq!(effort.as_str(), value);
            assert_eq!(serde_json::to_string(&effort).unwrap(), serde_json::to_string(value).unwrap());
        }
    }
}

/// Copyable configuration reference in the current interface language.
/// Only bundled templates are included; user credentials and recordings never enter this text.
pub fn configuration_spec(language: crate::i18n::Lang, selected: usize) -> String {
    use crate::i18n::Lang;
    let guide = match language {
        Lang::Ja => include_str!("asr-help/ja.md"),
        Lang::En => include_str!("asr-help/en.md"),
        Lang::Zh => include_str!("asr-help/zh.md"),
    };
    let profiles = AsrProfiles::default();
    let selected = selected.min(profiles.profiles.len() - 1);
    format!(
        "<dito_speech_model_config_spec>\n{}\n{}\n\n```json\n{}\n```\n</dito_speech_model_config_spec>",
        guide,
        AsrProfiles::NAMES[selected],
        serde_json::to_string_pretty(&profiles.profiles[selected]).expect("bundled JSON template")
    )
}

#[cfg(test)]
mod guide_tests {
    use super::*;
    use crate::i18n::Lang;
    #[test]
    fn copied_spec_follows_display_language_and_preserve_the_selected_template() {
        let profiles = AsrProfiles::default();
        for (language, title) in [
            (Lang::Ja, "# Dito 音声認識の接続設定仕様 v1"),
            (Lang::En, "# Dito speech model configuration specification v1"),
            (Lang::Zh, "# Dito 语音识别模型配置规范 v1"),
        ] {
            for selected in 0..3 {
                let copied = configuration_spec(language, selected);
                assert!(copied.starts_with(&format!("<dito_speech_model_config_spec>\n{title}")));
                assert!(copied.ends_with("</dito_speech_model_config_spec>"));
                assert_eq!(copied.matches("<dito_speech_model_config_spec>").count(), 1);
                for unwanted in [
                    "このガイドの使い方",
                    "Agent への依頼",
                    "How to use this guide",
                    "Task for the Agent",
                    "如何使用这份指南",
                    "给 Agent 的任务",
                ] {
                    assert!(!copied.contains(unwanted));
                }
                assert!(!copied.contains("FigCraft"));
                assert!(!copied.contains("design/"));
                assert!(copied.contains("{{api_key}}"));
                for (setting, code) in [
                    (crate::config::PreferredLanguage::Ja, "ja"),
                    (crate::config::PreferredLanguage::En, "en"),
                    (crate::config::PreferredLanguage::Zh, "zh"),
                ] {
                    assert_eq!(setting.code(), Some(code));
                    assert!(copied.contains(&format!("| `{code}` | `{code}` |")));
                }
                assert_eq!(crate::config::PreferredLanguage::Auto.code(), None);
                assert!(copied.contains("`null`"));
                assert!(copied.contains("`auto`"));
                assert!(copied.contains("`other`"));
                let json = copied.rsplit_once("```json\n").unwrap().1.split_once("\n```").unwrap().0;
                let template: Value = serde_json::from_str(json).unwrap();
                assert_eq!(template, profiles.profiles[selected]);
                assert_eq!(template["connection"]["model"], "");
            }
        }
    }
}

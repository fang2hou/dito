use std::collections::BTreeMap;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::multipart::{Form, Part};
use reqwest::{Client, Method, RequestBuilder, Url};
use serde_json::Value;

use super::{template, validate};
use crate::error::{ErrorKind, ProviderError, ProviderResult};
use crate::provider::{self, AsrRequest, Transcriber, Transcript};

const AUDIO_FILE_PLACEHOLDER: &str = "{{audio.file}}";

pub struct ConfiguredAsr {
    config: Value,
    key: String,
    client: Client,
}

impl ConfiguredAsr {
    pub fn new(config: Value, key: String) -> ProviderResult<Self> {
        validate::validate(&config)?;
        let client = Client::builder()
            .timeout(provider::default_timeout())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(transport_error)?;
        Ok(Self { config, key, client })
    }

    fn build_request(&self, wav: &[u8], language: Option<&str>) -> ProviderResult<RequestBuilder> {
        let c = &self.config;
        let audio = STANDARD.encode(wav);
        let vars = BTreeMap::from([
            ("model", validate::string(c, "/connection/model")?.to_owned()),
            ("api_key", self.key.clone()),
            ("audio.data_uri", format!("data:audio/wav;base64,{audio}")),
            ("audio.base64", audio),
        ]);
        let language = template::mapped_language(c, language);
        let r =
            template::expand(&c["request"], &vars, language.as_deref()).ok_or_else(|| validate::invalid("request"))?;

        let path = validate::string(&r, "/path")?;
        if !validate::is_relative_path(path) {
            return Err(validate::invalid("request.path: 相対パスを確認してください。"));
        }
        let url = format!("{}{}", validate::string(c, "/connection/base_url")?.trim_end_matches('/'), path);
        let mut url = Url::parse(&url).map_err(|_| validate::invalid("URL"))?;
        for (k, v) in r["query"].as_object().into_iter().flatten() {
            url.query_pairs_mut().append_pair(k, &scalar_text(v));
        }
        let method = Method::from_bytes(validate::string(c, "/request/method")?.as_bytes())
            .map_err(|_| validate::invalid("method"))?;

        let mut request = self.client.request(method, url);
        for (k, v) in r["headers"].as_object().into_iter().flatten() {
            if k.eq_ignore_ascii_case("content-type") {
                continue;
            }
            request = request.header(k, v.as_str().ok_or_else(|| validate::invalid("request.headers"))?);
        }
        if r["encoding"] == "multipart" {
            Ok(request.multipart(multipart_form(&r["fields"], wav)?))
        } else {
            Ok(request.json(&r["body"]))
        }
    }

    fn parse_response(&self, response: &Value) -> ProviderResult<Transcript> {
        let c = &self.config;
        let text = response
            .pointer(validate::string(c, "/response/text_pointer")?)
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::new(ErrorKind::BadResponse, "認識結果を取得できません。"))?
            .trim()
            .to_owned();
        if text.is_empty() {
            return Err(ProviderError::new(ErrorKind::NoSpeech, "音声を認識できませんでした。"));
        }
        let language = c["response"]["language_pointer"]
            .as_str()
            .and_then(|p| response.pointer(p))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(Transcript { text, language })
    }
}

#[async_trait]
impl Transcriber for ConfiguredAsr {
    async fn transcribe(&self, req: &AsrRequest<'_>) -> ProviderResult<Transcript> {
        let request = self.build_request(&req.clip.to_wav(), req.language)?;
        let response = request.send().await.map_err(transport_error)?;
        if !response.status().is_success() {
            return Err(ProviderError::from_status(response.status().as_u16(), "ASR request failed"));
        }
        let body: Value = response.json().await.map_err(transport_error)?;
        self.parse_response(&body)
    }
}

fn transport_error(e: reqwest::Error) -> ProviderError {
    ProviderError::from_reqwest(&e.without_url())
}

fn scalar_text(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())
}

fn multipart_form(fields: &Value, wav: &[u8]) -> ProviderResult<Form> {
    let mut form = Form::new();
    for (k, v) in fields.as_object().into_iter().flatten() {
        form = if v == AUDIO_FILE_PLACEHOLDER {
            let part =
                Part::bytes(wav.to_vec()).file_name("audio.wav").mime_str("audio/wav").map_err(transport_error)?;
            form.part(k.clone(), part)
        } else {
            form.text(k.clone(), scalar_text(v))
        };
    }
    Ok(form)
}

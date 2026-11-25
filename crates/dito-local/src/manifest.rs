//! Remote download URL manifest fetched on demand when starting a model download.
//!
//! File sizes and SHA-256 checksums stay pinned in `catalog.rs`; a manifest only supplies
//! replacement or mirror URLs so deployed builds recover when an upstream host moves files.

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MANIFEST_URL: &str = "https://fang2hou.github.io/dito/models/v1.json";
pub const MANIFEST_URL_ENV: &str = "DITO_MODEL_MANIFEST_URL";
pub const MANIFEST_VERSION: u32 = 1;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("cancelled")]
    Cancelled,
    #[error("HTTP {0}")]
    Http(u16),
    #[error("network: {0}")]
    Network(String),
    #[error("manifest exceeds {MAX_MANIFEST_BYTES} bytes")]
    TooLarge,
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("unsupported manifest version {0}")]
    UnsupportedVersion(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub models: BTreeMap<String, ModelUrls>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelUrls {
    #[serde(default)]
    pub files: BTreeMap<String, FileUrls>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileUrls {
    #[serde(default)]
    pub urls: Vec<String>,
}

/// Returns the manifest URL for production runs. Setting `DITO_MODEL_MANIFEST_URL` to an empty
/// string disables remote lookups; any non-empty value overrides the default GitHub Pages URL.
pub fn default_manifest_url() -> Option<String> {
    match std::env::var(MANIFEST_URL_ENV) {
        Ok(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Err(_) => Some(DEFAULT_MANIFEST_URL.to_owned()),
    }
}

pub(crate) fn is_loopback_http(url: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "http://127.0.0.1:",
        "http://127.0.0.1/",
        "http://localhost:",
        "http://localhost/",
        "http://[::1]:",
        "http://[::1]/",
    ];
    PREFIXES.iter().any(|prefix| url.starts_with(prefix))
}

fn is_allowed_remote_url(url: &str, allow_loopback_http: bool) -> bool {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty() && !rest.starts_with('/');
    }
    allow_loopback_http && is_loopback_http(url)
}

/// Every Hugging Face URL must pin a 40-character lowercase hex commit SHA so upstream branch
/// updates never break size or checksum verification.
#[cfg(test)]
pub(crate) fn is_pinned_hf_or_non_hf_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://huggingface.co/") else {
        return true;
    };
    let Some((_, after_resolve)) = rest.split_once("/resolve/") else {
        return false;
    };
    let Some((revision, _)) = after_resolve.split_once('/') else {
        return false;
    };
    revision.len() == 40 && revision.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|e| ManifestError::Json(e.to_string()))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedVersion(manifest.version));
        }
        Ok(manifest)
    }

    /// Candidate URLs for a catalog file: validated manifest URLs first, then `fallback` if not
    /// already listed.
    pub fn candidate_urls(
        &self,
        model_id: &str,
        file_name: &str,
        fallback: &str,
        allow_loopback_http: bool,
    ) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(entry) = self.models.get(model_id).and_then(|m| m.files.get(file_name)) {
            for candidate in &entry.urls {
                let candidate = candidate.trim();
                if is_allowed_remote_url(candidate, allow_loopback_http)
                    && !urls.iter().any(|existing| existing == candidate)
                {
                    urls.push(candidate.to_owned());
                }
            }
        }
        if !urls.iter().any(|existing| existing == fallback) {
            urls.push(fallback.to_owned());
        }
        urls
    }
}

pub async fn fetch(client: &reqwest::Client, url: &str, cancel: &CancellationToken) -> Result<Manifest, ManifestError> {
    let request = client.get(url).timeout(FETCH_TIMEOUT);
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ManifestError::Cancelled),
        r = request.send() => r.map_err(|e| ManifestError::Network(e.to_string()))?,
    };
    let status = response.status().as_u16();
    if status != 200 {
        return Err(ManifestError::Http(status));
    }
    if response.content_length().is_some_and(|len| len > MAX_MANIFEST_BYTES as u64) {
        return Err(ManifestError::TooLarge);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ManifestError::Cancelled),
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| ManifestError::Network(e.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
            return Err(ManifestError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Manifest::parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::default_catalog;

    #[test]
    fn parses_manifest_and_filters_unsafe_urls() {
        let raw = br#"{
            "version": 1,
            "extra_future_field": true,
            "models": {
                "qwen3-asr-1.7b": {
                    "files": {
                        "model.gguf": {
                            "urls": [
                                "http://evil.example/model.gguf",
                                "https://mirror.example/model.gguf",
                                "https://mirror.example/model.gguf",
                                "file:///etc/passwd"
                            ]
                        }
                    }
                }
            }
        }"#;
        let manifest = Manifest::parse(raw).unwrap();
        let candidates = manifest.candidate_urls("qwen3-asr-1.7b", "model.gguf", "https://default.example/m", false);
        assert_eq!(candidates, vec!["https://mirror.example/model.gguf", "https://default.example/m"]);
    }

    #[test]
    fn rejects_unsupported_manifest_version() {
        let err = Manifest::parse(br#"{"version": 2, "models": {}}"#).unwrap_err();
        assert!(matches!(err, ManifestError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_unpinned_huggingface_urls() {
        assert!(!is_pinned_hf_or_non_hf_url("https://huggingface.co/org/repo/resolve/main/model.gguf"));
        assert!(!is_pinned_hf_or_non_hf_url("https://huggingface.co/org/repo/resolve/v1.0/model.gguf"));
        assert!(is_pinned_hf_or_non_hf_url(
            "https://huggingface.co/org/repo/resolve/36a678687ba7d07a74ca70ccb0e36902e005fb80/model.gguf"
        ));
        assert!(is_pinned_hf_or_non_hf_url("https://github.com/org/repo/releases/download/v1/file.tar.gz"));
    }

    #[test]
    fn published_manifest_matches_default_catalog() {
        let raw = include_str!("../../../pages/models/v1.json");
        let manifest = Manifest::parse(raw.as_bytes()).expect("pages/models/v1.json must parse");
        for spec in default_catalog() {
            let entry = manifest
                .models
                .get(&spec.id)
                .unwrap_or_else(|| panic!("pages/models/v1.json is missing model {}", spec.id));
            for file in &spec.files {
                let urls = &entry
                    .files
                    .get(&file.name)
                    .unwrap_or_else(|| panic!("pages/models/v1.json is missing {}/{}", spec.id, file.name))
                    .urls;
                assert!(!urls.is_empty(), "{}/{} has no URLs", spec.id, file.name);
                assert!(
                    urls.contains(&file.url),
                    "pages/models/v1.json for {}/{} must include catalog URL {}",
                    spec.id,
                    file.name,
                    file.url
                );
            }
        }
        for (model_id, entry) in &manifest.models {
            for (file_name, file) in &entry.files {
                for url in &file.urls {
                    assert!(is_allowed_remote_url(url, false), "{model_id}/{file_name}: URL must be HTTPS: {url}");
                    assert!(
                        is_pinned_hf_or_non_hf_url(url),
                        "{model_id}/{file_name}: Hugging Face URL must pin a 40-char commit SHA: {url}"
                    );
                }
            }
        }
    }
}

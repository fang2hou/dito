//! Fitted heads: small linear classifiers over the hidden states of one prompt pass.
//!
//! A head is multinomial logistic regression with a temperature: `softmax((W * ((x − mean) / scale) + b) / T)`.
//! It is fitted by the bench (`mise run bench intent all`, or `fit` then `export` on stored runs) for one GGUF file and
//! one router prompt, and is valid for no other. The exported files in `heads/` are committed build inputs: a build
//! never refits.
//!
//! File layout, little-endian: the magic `DITOHEAD`, a `u32` format version, a `u32` header length, a JSON header,
//! then for each head in header order `mean[dim]`, `scale[dim]`, `weights[classes][dim]` and `bias[classes]` as
//! `f32`.

use std::sync::LazyLock;

use serde::Deserialize;

use super::{Intent, Reading, RewriteOp};

const MAGIC: &[u8; 8] = b"DITOHEAD";
const FORMAT_VERSION: u32 = 1;
const INTENT_HEAD: &str = "intent";
const REWRITE_OP_HEAD: &str = "rewrite.op";
/// Probabilities are clipped here before their logarithm is mixed, as in the fit.
const MIX_FLOOR: f32 = 1e-9;

/// The catalog ids with fitted heads and their files.
const BUNDLED: [(&str, &[u8]); 3] = [
    ("qwen3.5-2b-instruct", include_bytes!("heads/qwen3.5-2b-instruct.heads")),
    ("qwen3.5-4b-instruct", include_bytes!("heads/qwen3.5-4b-instruct.heads")),
    ("qwen3.5-9b-instruct", include_bytes!("heads/qwen3.5-9b-instruct.heads")),
];

/// The heads shipped for a catalog model, or `None` when it has none and the option logits decide alone.
pub fn bundled(model_id: &str) -> Option<&'static Heads> {
    static PARSED: LazyLock<Vec<Option<Heads>>> = LazyLock::new(|| {
        BUNDLED
            .iter()
            .map(|(id, bytes)| {
                Heads::parse(bytes).inspect_err(|error| tracing::error!(model = id, %error, "bundled heads")).ok()
            })
            .collect()
    });
    let index = BUNDLED.iter().position(|(id, _)| *id == model_id)?;
    PARSED[index].as_ref()
}

#[derive(Debug, thiserror::Error)]
pub enum HeadsError {
    #[error("not a heads file")]
    BadMagic,
    #[error("heads format version {0} is not supported")]
    Version(u32),
    #[error("heads header: {0}")]
    Header(String),
    #[error("heads file is {actual} bytes, its header describes {expected}")]
    Length { expected: usize, actual: usize },
    #[error(
        "head {head} has {actual} classes, this build routes {expected}: refit the heads on the current router prompt"
    )]
    Classes { head: String, expected: usize, actual: usize },
    #[error("head {head} does not classify the expected labels")]
    Labels { head: String },
    #[error("reading does not fit the heads: {0}")]
    Reading(String),
}

/// Where in the prompt a hidden state is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    /// The answer position: the last prompt token, whose output is the next-token distribution.
    Last,
    /// The mean over the tokens of the instruction.
    Span,
}

/// The residual stream entering block `layer`, pooled by `pool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "(u32, Pool)")]
pub struct Part {
    pub layer: u32,
    pub pool: Pool,
}

impl From<(u32, Pool)> for Part {
    fn from((layer, pool): (u32, Pool)) -> Self {
        Self { layer, pool }
    }
}

/// How the intent head is mixed with the option-logit readout of the same pass:
/// `p ∝ exp(weight * ln p_head + (1 − weight) * ln softmax(option_logits / temperature))`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Direct {
    pub weight: f32,
    pub temperature: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    /// Concatenated in this order to form the input.
    pub parts: Vec<Part>,
    classes: usize,
    mean: Vec<f32>,
    /// One row per class, already divided by the fitted scale.
    weights: Vec<f32>,
    bias: Vec<f32>,
    temperature: f32,
}

impl Head {
    fn dim(&self) -> usize {
        self.mean.len()
    }

    /// Class probabilities for the concatenation of `features`, one slice per part.
    fn probabilities(&self, features: &[&[f32]]) -> Result<Vec<f32>, HeadsError> {
        let total: usize = features.iter().map(|part| part.len()).sum();
        if total != self.dim() {
            return Err(HeadsError::Reading(format!("{total} feature values, the head reads {}", self.dim())));
        }
        let scores: Vec<f32> = self
            .weights
            .chunks_exact(self.dim())
            .zip(&self.bias)
            .map(|(row, bias)| {
                let centered = features.iter().flat_map(|part| part.iter()).zip(&self.mean).map(|(x, mean)| x - mean);
                row.iter().zip(centered).map(|(w, x)| w * x).sum::<f32>() + bias
            })
            .collect();
        debug_assert_eq!(scores.len(), self.classes);
        Ok(softmax(&scores, self.temperature))
    }
}

/// Every head fitted for one model file, and what they were fitted on.
#[derive(Debug, Clone, PartialEq)]
pub struct Heads {
    /// The catalog id.
    pub model: String,
    /// Lower-case hex SHA-256 of the GGUF file the heads were fitted on.
    pub gguf_sha256: String,
    /// Lower-case hex SHA-256 of [`super::fingerprint_text`] at fitting time.
    pub prompt_sha256: String,
    intent: Head,
    direct: Option<Direct>,
    rewrite_op: Option<Head>,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Header {
    model: String,
    gguf_sha256: String,
    prompt_sha256: String,
    direct: Option<Direct>,
    heads: Vec<HeadHeader>,
}

#[derive(Deserialize)]
struct HeadHeader {
    name: String,
    labels: Vec<String>,
    parts: Vec<Part>,
    temperature: f32,
    dim: usize,
}

impl HeadHeader {
    fn values(&self) -> usize {
        self.dim * 2 + self.labels.len() * self.dim + self.labels.len()
    }
}

impl Heads {
    pub fn parse(bytes: &[u8]) -> Result<Self, HeadsError> {
        let u32_at = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(HeadsError::BadMagic);
        }
        let version = u32_at(8).ok_or(HeadsError::BadMagic)?;
        if version != FORMAT_VERSION {
            return Err(HeadsError::Version(version));
        }
        let header_len = u32_at(12).ok_or(HeadsError::BadMagic)? as usize;
        let header_bytes = bytes
            .get(16..16 + header_len)
            .ok_or(HeadsError::Length { expected: 16 + header_len, actual: bytes.len() })?;
        let header: Header = serde_json::from_slice(header_bytes).map_err(|e| HeadsError::Header(e.to_string()))?;
        let expected = 16 + header_len + 4 * header.heads.iter().map(HeadHeader::values).sum::<usize>();
        if bytes.len() != expected {
            return Err(HeadsError::Length { expected, actual: bytes.len() });
        }

        let mut payload = bytes[16 + header_len..].as_chunks::<4>().0.iter().map(|value| f32::from_le_bytes(*value));
        let (mut intent, mut rewrite_op) = (None, None);
        for head in &header.heads {
            let labels: Vec<&str> = match head.name.as_str() {
                INTENT_HEAD => Intent::ALL.iter().map(|i| i.label()).collect(),
                REWRITE_OP_HEAD => RewriteOp::ALL.iter().map(|o| o.label()).collect(),
                other => return Err(HeadsError::Header(format!("unknown head {other}"))),
            };
            if head.labels.len() != labels.len() {
                return Err(HeadsError::Classes {
                    head: head.name.clone(),
                    expected: labels.len(),
                    actual: head.labels.len(),
                });
            }
            if head.labels != labels {
                return Err(HeadsError::Labels { head: head.name.clone() });
            }
            if head.parts.is_empty() || head.dim == 0 || head.dim % head.parts.len() != 0 || head.temperature <= 0.0 {
                return Err(HeadsError::Header(format!("head {} has an impossible shape", head.name)));
            }
            let mut take = |n: usize| payload.by_ref().take(n).collect::<Vec<f32>>();
            let (mean, scale) = (take(head.dim), take(head.dim));
            let mut weights = take(labels.len() * head.dim);
            let bias = take(labels.len());
            // Folded once here, so applying the head is one subtraction and one product per input value.
            for row in weights.chunks_exact_mut(head.dim) {
                row.iter_mut().zip(&scale).for_each(|(w, scale)| *w /= scale);
            }
            let parsed = Head {
                parts: head.parts.clone(),
                classes: labels.len(),
                mean,
                weights,
                bias,
                temperature: head.temperature,
            };
            match head.name.as_str() {
                INTENT_HEAD => intent = Some(parsed),
                _ => rewrite_op = Some(parsed),
            }
        }
        let intent = intent.ok_or_else(|| HeadsError::Header("no intent head".into()))?;
        let mut parts = intent.parts.clone();
        for part in rewrite_op.iter().flat_map(|head| &head.parts) {
            if !parts.contains(part) {
                parts.push(*part);
            }
        }
        Ok(Self {
            model: header.model,
            gguf_sha256: header.gguf_sha256,
            prompt_sha256: header.prompt_sha256,
            intent,
            direct: header.direct,
            rewrite_op,
            parts,
        })
    }

    /// Every hidden state the heads read, without repeats: what a request asks the runtime for.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn features<'a>(&self, head: &Head, reading: &'a Reading) -> Result<Vec<&'a [f32]>, HeadsError> {
        if reading.features.len() != self.parts.len() {
            return Err(HeadsError::Reading(format!(
                "{} hidden states, the heads read {}",
                reading.features.len(),
                self.parts.len()
            )));
        }
        // `parts` holds every part of every head, so the position is always found.
        Ok(head
            .parts
            .iter()
            .filter_map(|part| self.parts.iter().position(|p| p == part))
            .map(|index| reading.features[index].as_slice())
            .collect())
    }

    /// Intent probabilities in [`Intent::ALL`] order.
    pub(super) fn intent(&self, reading: &Reading) -> Result<Vec<f32>, HeadsError> {
        let head = self.intent.probabilities(&self.features(&self.intent, reading)?)?;
        let Some(direct) = self.direct else { return Ok(head) };
        if reading.option_logits.len() != head.len() {
            return Err(HeadsError::Reading(format!("{} option logits", reading.option_logits.len())));
        }
        let readout = softmax(&reading.option_logits, direct.temperature);
        let mixed: Vec<f32> = head
            .iter()
            .zip(&readout)
            .map(|(h, r)| direct.weight * h.max(MIX_FLOOR).ln() + (1.0 - direct.weight) * r.max(MIX_FLOOR).ln())
            .collect();
        Ok(softmax(&mixed, 1.0))
    }

    /// Rewrite-op probabilities in [`RewriteOp::ALL`] order, when the model has that head.
    pub(super) fn rewrite_op(&self, reading: &Reading) -> Result<Option<Vec<f32>>, HeadsError> {
        let Some(head) = &self.rewrite_op else { return Ok(None) };
        head.probabilities(&self.features(head, reading)?).map(Some)
    }
}

pub(super) fn softmax(scores: &[f32], temperature: f32) -> Vec<f32> {
    let max = scores.iter().copied().fold(f32::MIN, f32::max);
    let exp: Vec<f32> = scores.iter().map(|s| ((s - max) / temperature).exp()).collect();
    let total: f32 = exp.iter().sum();
    exp.iter().map(|e| e / total).collect()
}

pub(super) fn argmax(values: &[f32]) -> usize {
    values.iter().enumerate().fold((0, f32::MIN), |best, (i, &v)| if v > best.1 { (i, v) } else { best }).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(header: &str, payload: &[f32]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend(FORMAT_VERSION.to_le_bytes());
        bytes.extend((header.len() as u32).to_le_bytes());
        bytes.extend(header.as_bytes());
        payload.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        bytes
    }

    const LABELS: &str = r#"["polish","rewrite","translate","summarize","reply","ask","cancel","none"]"#;

    /// Two inputs taken from two parts. Class 0 scores `(x0 − 1) / 2`, class 1 scores `(x1 − 3) / 4 + 0.5`, the rest 0.
    fn payload() -> Vec<f32> {
        let mut values = vec![1.0, 3.0, 2.0, 4.0];
        values.extend([1.0, 0.0, 0.0, 1.0]);
        values.extend([0.0; 12]);
        values.extend([0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        values
    }

    fn header(direct: &str) -> String {
        format!(
            r#"{{"model":"m","gguf_sha256":"g","prompt_sha256":"p","direct":{direct},"heads":[{{"name":"intent","labels":{LABELS},"parts":[[3,"last"],[5,"span"]],"temperature":2.0,"dim":2}}]}}"#
        )
    }

    #[test]
    fn a_head_applies_mean_scale_weights_bias_and_temperature_in_file_order() {
        let heads = Heads::parse(&file(&header("null"), &payload())).unwrap();
        assert_eq!(heads.parts(), [Part { layer: 3, pool: Pool::Last }, Part { layer: 5, pool: Pool::Span }]);
        let reading = Reading { option_logits: vec![0.0; 8], features: vec![vec![5.0], vec![7.0]] };
        // Scores 2.0 and 1.5 at temperature 2.
        let expected = softmax(&[1.0, 0.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0);
        let got = heads.intent(&reading).unwrap();
        got.iter().zip(&expected).for_each(|(g, e)| assert!((g - e).abs() < 1e-6, "{got:?} vs {expected:?}"));
        assert!(heads.rewrite_op(&reading).unwrap().is_none());
        assert!(heads.intent(&Reading { option_logits: vec![0.0; 8], features: vec![vec![5.0]] }).is_err());
    }

    #[test]
    fn the_direct_mix_is_log_linear_in_the_head_and_the_readout() {
        let heads = Heads::parse(&file(&header(r#"{"weight":0.25,"temperature":0.5}"#), &payload())).unwrap();
        let reading = Reading {
            option_logits: vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            features: vec![vec![5.0], vec![7.0]],
        };
        let head = softmax(&[1.0, 0.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0);
        let readout = softmax(&[0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1.0);
        let mixed: Vec<f32> = head.iter().zip(&readout).map(|(h, r)| 0.25 * h.ln() + 0.75 * r.ln()).collect();
        let expected = softmax(&mixed, 1.0);
        let got = heads.intent(&reading).unwrap();
        got.iter().zip(&expected).for_each(|(g, e)| assert!((g - e).abs() < 1e-6, "{got:?} vs {expected:?}"));
        assert_eq!(argmax(&got), 2);
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let good = file(&header("null"), &payload());
        assert!(matches!(Heads::parse(&good[..good.len() - 4]), Err(HeadsError::Length { .. })));
        assert!(matches!(Heads::parse(b"DITOHEAX"), Err(HeadsError::BadMagic)));
        let swapped = header("null").replace("polish", "tidy");
        assert!(matches!(Heads::parse(&file(&swapped, &payload())), Err(HeadsError::Labels { .. })));
        // A file fitted for six intents has the right shape for itself and is still refused.
        let six = header("null").replace(r#","reply","ask""#, "");
        let mut six_payload = vec![1.0, 3.0, 2.0, 4.0, 1.0, 0.0, 0.0, 1.0];
        six_payload.extend([0.0; 8]);
        six_payload.extend([0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
        assert!(matches!(
            Heads::parse(&file(&six, &six_payload)),
            Err(HeadsError::Classes { actual: 6, expected: 8, .. })
        ));
    }
}

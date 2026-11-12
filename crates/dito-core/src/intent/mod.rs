//! Intent routing for voice commands: which tool a spoken instruction about the selected text asks for.
//!
//! The chat model the user already loaded decides, in one prompt pass and without generating a token. A runtime
//! answers a [`Request`] with a [`Reading`]: the logits of the option letters at the answer position, and the hidden
//! states the heads ask for. [`Heads`] fitted for that exact model file turn the reading into calibrated
//! probabilities; without them the option logits decide alone. Method, evidence and limits are in
//! `bench/INTENT.md`; how the pieces fit the app is in `docs/INTENT_ROUTING.md`.

use serde::{Deserialize, Serialize};

mod heads;
mod prompt;

pub use heads::{Direct, Head, Heads, HeadsError, Part, Pool, bundled};
pub use prompt::{OPTIONS, fingerprint_text, system_prompt, user_message};

/// A decision whose top probability reaches this runs without asking. The share that passes and how many of those are
/// right, per model, are in `bench/INTENT.md`.
pub const AUTO_THRESHOLD: f32 = 0.9;

/// What the user wants done with the selected text. Serialized as its `label` (a history row keeps decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    Polish,
    Rewrite,
    Translate,
    Summarize,
    /// The selected text was received and the user wants a response to send back.
    Reply,
    /// A question that expects an answer, about the selected text or anything else.
    Ask,
    Cancel,
    /// An action Dito cannot do, chit-chat, dictated content or garbled speech.
    None,
}

impl Intent {
    /// In option order: `ALL[i]` is the intent of `OPTIONS[i]` and class `i` of the intent head.
    pub const ALL: [Intent; 8] = [
        Intent::Polish,
        Intent::Rewrite,
        Intent::Translate,
        Intent::Summarize,
        Intent::Reply,
        Intent::Ask,
        Intent::Cancel,
        Intent::None,
    ];
    /// The number of intents: the length of `OPTIONS`, of the intent head and of [`Decision::probabilities`].
    pub const COUNT: usize = Self::ALL.len();

    pub fn label(self) -> &'static str {
        match self {
            Self::Polish => "polish",
            Self::Rewrite => "rewrite",
            Self::Translate => "translate",
            Self::Summarize => "summarize",
            Self::Reply => "reply",
            Self::Ask => "ask",
            Self::Cancel => "cancel",
            Self::None => "none",
        }
    }
}

/// The property a rewrite changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteOp {
    Expand,
    Shorten,
    Formal,
    Casual,
    Format,
    Paraphrase,
    Audience,
}

impl RewriteOp {
    /// In the class order of the `rewrite.op` head.
    pub const ALL: [RewriteOp; 7] = [
        RewriteOp::Expand,
        RewriteOp::Shorten,
        RewriteOp::Formal,
        RewriteOp::Casual,
        RewriteOp::Format,
        RewriteOp::Paraphrase,
        RewriteOp::Audience,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Shorten => "shorten",
            Self::Formal => "formal",
            Self::Casual => "casual",
            Self::Format => "format",
            Self::Paraphrase => "paraphrase",
            Self::Audience => "audience",
        }
    }
}

/// One prompt pass a runtime is asked for. Nothing is generated.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub system: &'static str,
    pub user: String,
    /// The escaped instruction as it stands in `user`. [`Pool::Span`] averages over the tokens of its last occurrence
    /// in the rendered prompt.
    pub span: String,
    /// Each must be one token of the model's vocabulary.
    pub options: &'static [&'static str],
    /// The hidden states to return, in this order.
    pub parts: Vec<Part>,
}

/// What a runtime read from one pass over a [`Request`].
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The logit of each option at the answer position, in option order.
    pub option_logits: Vec<f32>,
    /// One vector per requested part, in request order.
    pub features: Vec<Vec<f32>>,
}

/// The request for one spoken instruction. `heads` are the ones [`decide`] will be given.
pub fn request(instruction: &str, heads: Option<&Heads>) -> Request {
    Request {
        system: system_prompt(),
        user: user_message(instruction),
        span: prompt::escape(instruction),
        options: &OPTIONS,
        parts: heads.map(|heads| heads.parts().to_vec()).unwrap_or_default(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub intent: Intent,
    /// The probability of `intent`.
    pub confidence: f32,
    /// In [`Intent::ALL`] order.
    pub probabilities: [f32; Intent::COUNT],
    /// Set for a rewrite when the model's heads include `rewrite.op`, with its probability.
    pub rewrite_op: Option<(RewriteOp, f32)>,
}

impl Decision {
    /// Whether the decision may run without asking the user.
    pub fn is_confident(&self) -> bool {
        self.confidence >= AUTO_THRESHOLD
    }

    /// Every intent with its probability, most likely first: the candidates to offer when not confident.
    pub fn ranked(&self) -> [(Intent, f32); Intent::COUNT] {
        let mut ranked = std::array::from_fn(|i| (Intent::ALL[i], self.probabilities[i]));
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
    }
}

/// Turn a reading into a decision. `heads` must be the ones the request was built with.
pub fn decide(heads: Option<&Heads>, reading: &Reading) -> Result<Decision, HeadsError> {
    let probabilities: [f32; Intent::COUNT] = match heads {
        Some(heads) => heads.intent(reading)?,
        None => heads::softmax(&reading.option_logits, 1.0),
    }
    .try_into()
    .map_err(|p: Vec<f32>| HeadsError::Reading(format!("{} option logits, expected {}", p.len(), Intent::COUNT)))?;
    let top = heads::argmax(&probabilities);
    let intent = Intent::ALL[top];
    let rewrite_op = match heads {
        Some(heads) if intent == Intent::Rewrite => heads.rewrite_op(reading)?.map(|p| {
            let top = heads::argmax(&p);
            (RewriteOp::ALL[top], p[top])
        }),
        _ => None,
    };
    Ok(Decision { intent, confidence: probabilities[top], probabilities, rewrite_op })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_heads_the_option_logits_decide() {
        let reading = Reading { option_logits: vec![0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0], features: Vec::new() };
        let decision = decide(None, &reading).unwrap();
        assert_eq!(decision.intent, Intent::Translate);
        assert!(decision.is_confident());
        assert_eq!(decision.ranked()[0].0, Intent::Translate);
        assert!((decision.probabilities.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(decide(None, &Reading { option_logits: vec![0.0; 7], features: Vec::new() }).is_err());
    }

    #[test]
    fn every_bundled_model_has_heads_that_fit_the_prompt_options() {
        for id in ["qwen3.5-2b-instruct", "qwen3.5-4b-instruct", "qwen3.5-9b-instruct"] {
            let heads = bundled(id).unwrap_or_else(|| panic!("no heads bundled for {id}"));
            assert_eq!(heads.model, id);
            assert!(!heads.parts().is_empty());
            assert_eq!(request("x", Some(heads)).parts, heads.parts());
        }
        assert!(bundled("qwen3-asr-1.7b").is_none());
    }
}

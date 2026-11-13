use std::future::{Future, ready};
use std::pin::Pin;

use dito_core::error::ProviderResult;
use dito_core::intent::{Decision, Intent};
use dito_core::provider::Router;
use dito_core::provider::mock::decision;

const CONFIDENT: f32 = 0.97;

const UNCLEAR: &[&str] = &["どれ", "unclear"];
const CANCEL: &[&str] = &["キャンセル", "cancel"];
const REPLY: &[&str] = &["返信", "返事", "回复", "回一下", "reply", "write back"];
const ACTION: &[&str] = &["送って", "送信", "保存", "開いて", "検索", "send", "save", "open", "search"];
const TRANSLATE: &[&str] = &["翻訳", "英語", "translate"];
const SUMMARIZE: &[&str] = &["要約", "まとめ", "summarize"];
const REWRITE: &[&str] = &["書き換え", "丁寧", "短く", "rewrite"];
const POLISH: &[&str] = &["整形", "直して", "polish"];
const QUESTION: &[&str] = &["なぜ", "what", "how"];

/// Routes by keywords of the instruction, so every state of command mode is reachable without a model. An empty or
/// unknown instruction, and one that says "どれ" or "unclear", is not confident: the HUD shows the choices. An action
/// Dito cannot do ("送って", "send") is `none`: the capsule closes.
pub struct KeywordRouter;

fn has(text: &str, words: &[&str]) -> bool {
    words.iter().any(|word| text.contains(word))
}

fn route(instruction: &str) -> Decision {
    let text = instruction.trim().to_lowercase();
    let intent = if text.is_empty() || has(&text, UNCLEAR) {
        None
    } else if has(&text, CANCEL) {
        Some(Intent::Cancel)
    } else if has(&text, REPLY) {
        Some(Intent::Reply)
    } else if has(&text, TRANSLATE) {
        Some(Intent::Translate)
    } else if has(&text, SUMMARIZE) {
        Some(Intent::Summarize)
    } else if has(&text, REWRITE) {
        Some(Intent::Rewrite)
    } else if has(&text, POLISH) {
        Some(Intent::Polish)
    } else if text.ends_with(['？', '?']) || has(&text, QUESTION) {
        Some(Intent::Ask)
    } else if has(&text, ACTION) {
        Some(Intent::None)
    } else {
        None
    };
    match intent {
        Some(intent) => decision(intent, CONFIDENT),
        // In option order: polish, rewrite, translate, summarize, reply, ask, cancel, none.
        None => Decision {
            intent: Intent::Summarize,
            confidence: 0.62,
            probabilities: [0., 0.11, 0.27, 0.62, 0., 0., 0., 0.],
            rewrite_op: None,
        },
    }
}

// Written out by hand: dito-app has no async-trait dependency.
impl Router for KeywordRouter {
    fn decide<'a, 'b, 'c>(
        &'a self,
        instruction: &'b str,
    ) -> Pin<Box<dyn Future<Output = ProviderResult<Decision>> + Send + 'c>>
    where
        'a: 'c,
        'b: 'c,
        Self: 'c,
    {
        Box::pin(ready(Ok(route(instruction))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_reach_reply_ask_and_none() {
        let intent = |text| route(text).intent;
        for text in ["返信して", "返事を書いて", "回复一下", "这个回一下", "reply to this", "write back"]
        {
            assert_eq!(intent(text), Intent::Reply, "{text}");
        }
        assert_eq!(intent("これは何ですか？"), Intent::Ask);
        assert_eq!(intent("佐藤さんに送って"), Intent::None);
        assert!(route("佐藤さんに送って").is_confident());
    }
}

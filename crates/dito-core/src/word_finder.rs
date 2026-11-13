//! Finds words worth adding to the wordbook in text the user pasted or chose as a file. The polish model reads the
//! text block by block and lists terms; only a term written in its block, character for character, is kept, so a
//! model can leave terms out but cannot invent one (ADR-0012). `design:settings/wordbook/find-words`.

mod prompt;
mod reply;
mod source;

use std::collections::HashSet;

use tokio::sync::watch;

pub use prompt::is_request;
pub use reply::{OTHER_AI_PROMPT, Reply, ReplyError, read_reply};
pub use source::{EXTENSIONS, SourceError, read_source};

use crate::controller::HudState;
use crate::error::ProviderResult;
use crate::provider::{PolishRequest, Polisher};

/// About a paragraph or two: short enough for a small local model to read closely, long enough to keep a term with
/// the sentence around it.
const BLOCK_CHARS: usize = 800;
const TERM_CHARS: std::ops::RangeInclusive<usize> = 2..=40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub term: String,
    /// Starts selected: it is shaped like a name or the text uses it more than once.
    pub likely: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

/// Candidates in the order the text first uses them, without the words in `known`. Waits before each block while
/// `hud` shows a dictation or a command at work, so the model is free for it.
pub async fn find_words(
    polisher: &dyn Polisher,
    text: &str,
    known: &[String],
    hud: &mut watch::Receiver<HudState>,
    progress: &watch::Sender<Progress>,
) -> ProviderResult<Vec<Candidate>> {
    let blocks = blocks(text);
    let mut seen: HashSet<String> = known.iter().map(|word| word.to_lowercase()).collect();
    let mut found = Vec::new();
    progress.send_replace(Progress { done: 0, total: blocks.len() });
    for (index, block) in blocks.iter().enumerate() {
        // A closed channel means the controller is gone with the app, so there is no dictation to wait for.
        let _ = hud.wait_for(|state| !at_work(state)).await;
        let request = prompt::request(block);
        let reply = polisher
            .polish(&PolishRequest { system: prompt::SYSTEM, examples: prompt::EXAMPLES, text: &request })
            .await?;
        for term in terms_in(&reply, block) {
            if seen.insert(term.to_lowercase()) {
                let likely = is_name_shaped(&term) || text.matches(term.as_str()).nth(1).is_some();
                found.push(Candidate { term, likely });
            }
        }
        progress.send_replace(Progress { done: index + 1, total: blocks.len() });
    }
    Ok(found)
}

fn at_work(state: &HudState) -> bool {
    matches!(
        state,
        HudState::Listening { .. }
            | HudState::Transcribing
            | HudState::Polishing
            | HudState::Thinking
            | HudState::Working
    )
}

/// Whole paragraphs packed up to [`BLOCK_CHARS`]; a longer paragraph is cut after a sentence where it can be.
fn blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for piece in text.lines().map(str::trim).filter(|line| !line.is_empty()).flat_map(sentences_within_limit) {
        let chars = piece.chars().count();
        if current_chars > 0 && current_chars + 1 + chars > BLOCK_CHARS {
            out.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        if current_chars > 0 {
            current.push('\n');
            current_chars += 1;
        }
        current.push_str(piece);
        current_chars += chars;
    }
    if current_chars > 0 {
        out.push(current);
    }
    out
}

fn sentences_within_limit(paragraph: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = paragraph;
    while rest.chars().count() > BLOCK_CHARS {
        let limit = rest.char_indices().nth(BLOCK_CHARS).map_or(rest.len(), |(at, _)| at);
        let cut = rest[..limit]
            .char_indices()
            .filter(|(_, c)| matches!(c, '。' | '．' | '.' | '！' | '!' | '？' | '?' | '；' | ';'))
            .map(|(at, c)| at + c.len_utf8())
            .next_back()
            .unwrap_or(limit);
        let (head, tail) = rest.split_at(cut);
        out.push(head.trim());
        rest = tail.trim_start();
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// The lines of a model reply that are terms of `block`: list markers and quotes around a line are dropped, and a
/// line the block does not contain is not a term.
fn terms_in(reply: &str, block: &str) -> Vec<String> {
    reply
        .lines()
        .map(|line| {
            let line = line.trim().trim_start_matches(['-', '•', '*', '・']).trim_start();
            let unnumbered = line.trim_start_matches(|c: char| c.is_ascii_digit());
            let line = match unnumbered.strip_prefix(['.', ')', '、']) {
                Some(rest) if unnumbered.len() < line.len() => rest,
                _ => line,
            };
            line.trim().trim_matches(['"', '\'', '`', '「', '」', '“', '”'])
        })
        .filter(|term| is_term(term))
        .filter(|term| block.contains(term))
        .map(str::to_string)
        .collect()
}

/// One word or one short name, not a number and not a sentence.
fn is_term(term: &str) -> bool {
    TERM_CHARS.contains(&term.chars().count())
        && term.chars().any(char::is_alphabetic)
        && !term.contains(['。', '、', '，', ',', '；', ';', '！', '!', '？', '?', '\n'])
}

/// Spelled the way names and identifiers are: a capital letter, a digit beside letters, or a joining mark inside.
pub fn is_name_shaped(term: &str) -> bool {
    let letters = term.chars().any(|c| c.is_ascii_alphabetic());
    let inner = term.trim_matches(|c: char| !c.is_alphanumeric());
    letters
        && (term.chars().any(|c| c.is_ascii_uppercase())
            || term.chars().any(|c| c.is_ascii_digit())
            || inner.chars().any(|c| matches!(c, '-' | '_' | '.' | '/' | '+' | '#')))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::config::RecordMode;
    use crate::provider::mock::MockPolisher;

    fn idle() -> (watch::Sender<HudState>, watch::Receiver<HudState>) {
        watch::channel(HudState::Hidden)
    }

    async fn find(polisher: &dyn Polisher, text: &str, known: &[&str]) -> Vec<Candidate> {
        let known: Vec<String> = known.iter().map(|word| word.to_string()).collect();
        let (_hud, mut hud_rx) = idle();
        let (progress, _) = watch::channel(Progress::default());
        find_words(polisher, text, &known, &mut hud_rx, &progress).await.expect("a search that succeeds")
    }

    #[test]
    fn only_lines_written_in_the_block_are_terms() {
        let block = "来週は Tokio を 1.40 に上げ、ファブリカム商事の検証環境を移します。";
        let reply = "- Tokio\n2. ファブリカム商事\n「検証環境」\nKubernetes\nNONE\n1.40\n来週は Tokio を 1.40 に上げ、ファブリカム商事の検証環境を移します。";
        assert_eq!(terms_in(reply, block), ["Tokio", "ファブリカム商事", "検証環境"]);
    }

    #[test]
    fn a_term_that_starts_with_digits_keeps_them() {
        assert_eq!(terms_in("3M\n1. 2FA", "3M と 2FA"), ["3M", "2FA"]);
    }

    #[test]
    fn long_text_is_cut_into_blocks_at_paragraphs_then_sentences_and_loses_nothing() {
        let sentence = "これは四十文字ほどの長さを持つ一つの文であり、句点で終わるものです。";
        let long_paragraph = sentence.repeat(60);
        let text = format!("short one\n\n{long_paragraph}\nlast line");
        let blocks = blocks(&text);
        assert!(blocks.len() > 2);
        assert!(blocks.iter().all(|block| block.chars().count() <= BLOCK_CHARS));
        assert!(blocks[1..blocks.len() - 1].iter().all(|block| block.ends_with('。')));
        let joined: String = blocks.concat().replace('\n', "");
        assert_eq!(joined, text.replace('\n', ""));
    }

    #[tokio::test]
    async fn candidates_skip_known_words_and_repeats_and_mark_the_likely_ones() {
        let text = "Northwind の検証環境を Tokio に移す。\n\n検証環境は northwind が持つ。負荷試験は来週。";
        let polisher = MockPolisher::default().script([Ok("Northwind\n検証環境\nTokio\n負荷試験\nnorthwind".into())]);
        let found = find(&polisher, text, &["tokio"]).await;
        assert_eq!(
            found,
            [
                Candidate { term: "Northwind".into(), likely: true },
                Candidate { term: "検証環境".into(), likely: true },
                Candidate { term: "負荷試験".into(), likely: false },
            ]
        );
    }

    struct Recording(Mutex<Vec<String>>);

    #[async_trait]
    impl Polisher for Recording {
        async fn polish(&self, req: &PolishRequest<'_>) -> ProviderResult<String> {
            self.0.lock().expect("calls mutex poisoned").push(req.text.to_string());
            Ok("NONE".into())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_block_waits_until_the_dictation_at_work_is_over() {
        let polisher = Recording(Mutex::default());
        let (hud, mut hud_rx) = watch::channel(HudState::Listening { mode: RecordMode::Hold });
        let (progress, progress_rx) = watch::channel(Progress::default());
        let known = Vec::new();
        let search = find_words(&polisher, "some text", &known, &mut hud_rx, &progress);
        tokio::pin!(search);
        assert!(tokio::time::timeout(std::time::Duration::from_secs(5), &mut search).await.is_err());
        assert!(polisher.0.lock().expect("calls mutex poisoned").is_empty());
        hud.send_replace(HudState::Inserted);
        assert_eq!(search.await.expect("a search that succeeds"), []);
        assert_eq!(*progress_rx.borrow(), Progress { done: 1, total: 1 });
    }
}

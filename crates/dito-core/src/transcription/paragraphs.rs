//! Grouping recognized utterances into paragraphs.

use super::Speaker;
use crate::i18n::Lang;

/// A pause at least this long between utterances ends the paragraph.
pub const PARAGRAPH_GAP_MS: u64 = 2_000;
/// Once a paragraph is this long, the next sentence end closes it, so a speaker who never pauses is still polished
/// a few sentences at a time.
pub const PARAGRAPH_MIN_CHARS: usize = 80;
/// A paragraph without any sentence end also ends once it is this long.
pub const PARAGRAPH_MAX_CHARS: usize = 280;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Utterance {
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// The language the engine reported for the utterance, which its spoken forms were matched in.
    pub lang: Option<Lang>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Paragraph {
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// The language of its latest utterance, which its polished text is restored in.
    pub lang: Option<Lang>,
}

#[derive(Debug, Default)]
pub(super) struct Paragrapher {
    open: Option<Paragraph>,
}

impl Paragrapher {
    /// Returns the paragraphs the utterance closed, oldest first. The first of them is the paragraph that was open
    /// before the push, if one was.
    pub fn push(&mut self, u: Utterance) -> Vec<Paragraph> {
        let mut closed = Vec::new();
        match &mut self.open {
            Some(p)
                if p.speaker == u.speaker
                    && u.start_ms < p.end_ms + PARAGRAPH_GAP_MS
                    && p.text.chars().count() < PARAGRAPH_MAX_CHARS =>
            {
                join(&mut p.text, &u.text);
                p.end_ms = p.end_ms.max(u.end_ms);
                p.lang = u.lang;
            }
            _ => {
                closed.extend(self.open.replace(Paragraph {
                    speaker: u.speaker,
                    start_ms: u.start_ms,
                    end_ms: u.end_ms,
                    lang: u.lang,
                    text: u.text,
                }));
            }
        }
        while let Some(p) = self.open.take() {
            let Some(cut) = sentence_cut(&p.text) else {
                self.open = Some(p);
                break;
            };
            let (head, rest) = p.text.split_at(cut);
            let rest = rest.trim_start();
            // Without word timings, time is shared out by length.
            let total = p.text.chars().count().max(1) as u64;
            let at = p.start_ms + (p.end_ms - p.start_ms) * head.chars().count() as u64 / total;
            if !rest.is_empty() {
                self.open = Some(Paragraph {
                    speaker: p.speaker,
                    lang: p.lang,
                    start_ms: at,
                    end_ms: p.end_ms,
                    text: rest.into(),
                });
            }
            closed.push(Paragraph {
                speaker: p.speaker,
                lang: p.lang,
                start_ms: p.start_ms,
                end_ms: at,
                text: head.into(),
            });
        }
        closed
    }

    pub fn open(&self) -> Option<&Paragraph> {
        self.open.as_ref()
    }

    /// Closes the open paragraph once nobody has spoken for the paragraph gap.
    pub fn close_if_idle(&mut self, now_ms: u64) -> Option<Paragraph> {
        if self.open.as_ref().is_some_and(|p| now_ms >= p.end_ms + PARAGRAPH_GAP_MS) { self.open.take() } else { None }
    }

    pub fn take(&mut self) -> Option<Paragraph> {
        self.open.take()
    }
}

/// The byte offset just past the first sentence end at or after `PARAGRAPH_MIN_CHARS`, closing quotes and brackets
/// included. A period of a spaced language ends a sentence only before a space or the end, so `3.5` stays whole.
fn sentence_cut(text: &str) -> Option<usize> {
    let mut chars = text.char_indices().enumerate().peekable();
    while let Some((n, (i, c))) = chars.next() {
        let next = chars.peek().map(|(_, (_, c))| *c);
        let ends = match c {
            '。' | '！' | '？' | '．' => true,
            '.' | '!' | '?' => next.is_none_or(char::is_whitespace),
            _ => false,
        };
        if !ends || n + 1 < PARAGRAPH_MIN_CHARS {
            continue;
        }
        let mut end = i + c.len_utf8();
        while let Some((_, (j, q))) = chars.peek().copied() {
            if !matches!(q, '」' | '』' | '）' | ')' | '"' | '”' | '’') {
                break;
            }
            end = j + q.len_utf8();
            chars.next();
        }
        return Some(end);
    }
    None
}

/// Japanese and Chinese run together; a space separates two words of a spaced language.
pub(super) fn join(text: &mut String, next: &str) {
    let spaced = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ',' | '!' | '?'));
    if spaced(text.chars().last()) && next.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        text.push(' ');
    }
    text.push_str(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(speaker: Speaker, start_ms: u64, end_ms: u64, text: &str) -> Utterance {
        Utterance { speaker, start_ms, end_ms, text: text.into(), lang: None }
    }

    #[test]
    fn a_pause_a_new_speaker_or_a_long_paragraph_starts_the_next_paragraph() {
        let mut p = Paragrapher::default();
        assert!(p.push(u(Speaker::Others, 0, 3_000, "まず")).is_empty());
        assert!(p.push(u(Speaker::Others, 3_500, 5_000, "リリースです")).is_empty());
        assert_eq!(p.open().unwrap().text, "まずリリースです");

        let [first] = &p.push(u(Speaker::Others, 7_000, 8_000, "次に"))[..] else {
            panic!("a 2 s pause closes the paragraph")
        };
        assert_eq!((first.start_ms, first.end_ms), (0, 5_000));

        assert_eq!(p.push(u(Speaker::Me, 8_200, 9_000, "はい")).len(), 1);

        p.push(u(Speaker::Me, 9_100, 9_500, &"あ".repeat(PARAGRAPH_MAX_CHARS)));
        assert_eq!(p.push(u(Speaker::Me, 9_600, 9_900, "続き")).len(), 1);
    }

    #[test]
    fn past_the_minimum_length_each_sentence_end_closes_a_paragraph() {
        let mut p = Paragrapher::default();
        let sentence = format!("{}。", "あ".repeat(39));
        assert!(p.push(u(Speaker::Others, 0, 4_000, &sentence)).is_empty(), "40 characters stay open");
        // One long utterance carries the paragraph past 80 characters, then past another sentence end.
        let long = format!("{sentence}{sentence}「{}。」{}", "い".repeat(78), "う".repeat(10));
        let closed = p.push(u(Speaker::Others, 4_000, 20_000, &long));
        assert_eq!(closed.len(), 2);
        assert_eq!(closed[0].text, format!("{sentence}{sentence}"), "the sentence that reaches 80 characters");
        assert!(closed[1].text.starts_with(&sentence));
        assert!(closed[1].text.ends_with("。」"), "a closing bracket stays with its sentence");
        assert_eq!(closed[0].end_ms, closed[1].start_ms);
        let open = p.open().unwrap();
        assert_eq!(open.text, "う".repeat(10));
        assert_eq!((closed[1].end_ms, open.end_ms), (open.start_ms, 20_000));

        let mut en = Paragrapher::default();
        let text = format!("{} costs 3.5 dollars. Then", "word ".repeat(16));
        let [closed] = &en.push(u(Speaker::Others, 0, 1_000, &text))[..] else { panic!("one sentence end") };
        assert!(closed.text.ends_with("3.5 dollars."));
        assert_eq!(en.open().unwrap().text, "Then");
    }

    #[test]
    fn silence_closes_the_open_paragraph() {
        let mut p = Paragrapher::default();
        p.push(u(Speaker::Others, 0, 1_000, "hello"));
        assert!(p.close_if_idle(2_500).is_none());
        assert_eq!(p.close_if_idle(3_000).unwrap().text, "hello");
        assert!(p.open().is_none());
    }

    #[test]
    fn spaced_languages_get_a_space_between_utterances() {
        let mut text = String::from("We ship on Thursday.");
        join(&mut text, "Tests are done");
        assert_eq!(text, "We ship on Thursday. Tests are done");
        let mut ja = String::from("木曜です。");
        join(&mut ja, "テストは");
        assert_eq!(ja, "木曜です。テストは");
    }
}

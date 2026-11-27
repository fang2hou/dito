//! The language a meeting is held in, decided by code from the transcript and handed to the notes requests as data,
//! so the model never guesses it and the interface language never stands in for it.
//!
//! Each utterance is classified on its own by the scripts it uses, then weighted by how long it lasted, which does
//! not depend on how many letters a language spends on a word. A label the text does not support is worse than none:
//! a wrong `Known` makes the model translate the whole meeting, so French, Mongolian and anything else that cannot be
//! told apart reliably stays `Unknown`, and too little text gives no label at all.

use crate::i18n::Lang;
use crate::transcription::Utterance;

/// The language the notes are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingLang {
    Known(Lang),
    /// Enough text, in a language the scripts cannot name reliably: the notes follow a sample of it.
    Unknown,
}

impl MeetingLang {
    pub fn known(self) -> Option<Lang> {
        match self {
            Self::Known(lang) => Some(lang),
            Self::Unknown => None,
        }
    }
}

/// Which request asks. A hint reads one minute and never moves a settled language; a memo reads minutes and may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judge {
    Hint,
    Memo,
}

/// What one request names, and what the notes keep for the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LangChoice {
    /// The language the request names. `None`: too little text and nothing earlier to go by, so no claim is made.
    pub request: Option<MeetingLang>,
    /// The language the notes carry on (`Notes::lang`): only one the transcript itself showed, never the prior.
    pub settled: Option<Lang>,
}

/// Below this much speech, by time or by text, the window says too little to decide.
const ENOUGH_MS: u64 = 15_000;
const ENOUGH_CJK_CHARS: usize = 40;
const ENOUGH_WORDS: usize = 20;

/// A memo moves a settled language only once the new one holds this share of the speech, in percent.
const SWITCH_SHARE: u64 = 70;

/// The language of `utterances` for one request. `previous` is the language the notes settled on earlier; `prior`
/// is the language the user set as the one they speak (`general.preferred_language`), used only while there is too
/// little text and nothing settled.
pub fn meeting_lang(
    utterances: &[&Utterance],
    previous: Option<Lang>,
    prior: Option<Lang>,
    judge: Judge,
) -> LangChoice {
    let keep = |lang: Lang| LangChoice { request: Some(MeetingLang::Known(lang)), settled: Some(lang) };
    if let (Some(lang), Judge::Hint) = (previous, judge) {
        return keep(lang);
    }
    let tally = Tally::of(utterances);
    let Some((leader, weight)) = tally.leader().filter(|_| tally.enough()) else {
        return match previous {
            Some(lang) => keep(lang),
            None => LangChoice { request: prior.map(MeetingLang::Known), settled: None },
        };
    };
    match previous {
        Some(lang) if leader != MeetingLang::Known(lang) && weight * 100 < SWITCH_SHARE * tally.total => keep(lang),
        _ => LangChoice { request: Some(leader), settled: leader.known() },
    }
}

/// The language one utterance is in, or `None` when it holds no letters.
pub fn classify(text: &str) -> Option<MeetingLang> {
    let s = Scripts::of(text);
    // One CJK or Hangul character carries about as much as three Latin or Cyrillic letters.
    let scores = [(s.kana + s.han) * 3, s.hangul * 3, s.latin, s.cyrillic];
    let top = *scores.iter().max()?;
    if top == 0 {
        return None;
    }
    Some(if top == scores[0] {
        // A kana or two inside Chinese text does not make it Japanese.
        if s.kana * 10 >= s.kana + s.han { MeetingLang::Known(Lang::Ja) } else { MeetingLang::Known(Lang::Zh) }
    } else if top == scores[1] {
        MeetingLang::Known(Lang::Ko)
    } else if top == scores[2] && is_english(text) {
        MeetingLang::Known(Lang::En)
    } else {
        MeetingLang::Unknown
    })
}

/// The utterance the request shows as a sample of the meeting's language: the longest one in that language, the
/// newest of equals. With no language named, the longest one in an unnamed language, else the longest of all.
pub fn language_sample<'a>(utterances: &[&'a Utterance], lang: Option<MeetingLang>) -> Option<&'a Utterance> {
    let longest = |keep: &dyn Fn(&Utterance) -> bool| {
        utterances.iter().copied().filter(|u| keep(u)).max_by_key(|u| u.text.chars().count())
    };
    match lang {
        Some(MeetingLang::Known(lang)) => longest(&|u| classify(&u.text) == Some(MeetingLang::Known(lang))),
        Some(MeetingLang::Unknown) | None => {
            longest(&|u| classify(&u.text) == Some(MeetingLang::Unknown)).or_else(|| longest(&|_| true))
        }
    }
}

/// Words common in English and rare as words of the other Latin-script languages a meeting is likely held in.
const ENGLISH_WORDS: &[&str] = &[
    "the", "and", "to", "of", "is", "we", "that", "it", "for", "you", "this", "with", "have", "are", "what", "can",
    "our", "they", "about", "just", "think", "know", "would", "should", "there", "been", "were", "your", "yes", "yeah",
    "it's", "i'm", "don't", "let's", "that's", "we're", "you're",
];

/// At least this share of the words, in percent, are common English words.
const ENGLISH_SHARE: usize = 8;

fn is_english(text: &str) -> bool {
    let words: Vec<String> = text
        .split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '\u{2019}'))
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('\u{2019}', "'").to_lowercase())
        .collect();
    let common = words.iter().filter(|w| ENGLISH_WORDS.contains(&w.as_str())).count();
    !words.is_empty() && common * 100 >= ENGLISH_SHARE * words.len()
}

#[derive(Default)]
struct Scripts {
    kana: usize,
    han: usize,
    hangul: usize,
    latin: usize,
    cyrillic: usize,
    words: usize,
}

impl Scripts {
    fn of(text: &str) -> Self {
        let mut s = Self::default();
        let mut in_word = false;
        for c in text.chars() {
            let mut word_letter = false;
            match c {
                // The middle dot U+30FB is left out: Chinese text uses it too.
                '\u{3041}'..='\u{309f}'
                | '\u{30a0}'..='\u{30fa}'
                | '\u{30fc}'..='\u{30ff}'
                | '\u{31f0}'..='\u{31ff}' => {
                    s.kana += 1;
                }
                '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{f900}'..='\u{faff}' => s.han += 1,
                '\u{ac00}'..='\u{d7af}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' => s.hangul += 1,
                '\u{0400}'..='\u{04ff}' => {
                    s.cyrillic += 1;
                    word_letter = true;
                }
                c if c.is_alphabetic() && (c as u32) < 0x250 => {
                    s.latin += 1;
                    word_letter = true;
                }
                _ => {}
            }
            if word_letter && !in_word {
                s.words += 1;
            }
            in_word = word_letter;
        }
        s
    }
}

/// Speech time per language over a window.
struct Tally {
    /// Ja, Zh, Ko, En, then unnamed: a fixed order, so a tie always falls the same way.
    weights: [(MeetingLang, u64); 5],
    total: u64,
    cjk_chars: usize,
    words: usize,
}

impl Tally {
    fn of(utterances: &[&Utterance]) -> Self {
        let mut tally = Self {
            weights: [
                (MeetingLang::Known(Lang::Ja), 0),
                (MeetingLang::Known(Lang::Zh), 0),
                (MeetingLang::Known(Lang::Ko), 0),
                (MeetingLang::Known(Lang::En), 0),
                (MeetingLang::Unknown, 0),
            ],
            total: 0,
            cjk_chars: 0,
            words: 0,
        };
        for u in utterances {
            let Some(lang) = classify(&u.text) else { continue };
            // Words still being heard end where they start, so every utterance counts for at least a second.
            let weight = u.end_ms.saturating_sub(u.start_ms).max(1_000);
            if let Some(slot) = tally.weights.iter_mut().find(|(l, _)| *l == lang) {
                slot.1 += weight;
            }
            tally.total += weight;
            let s = Scripts::of(&u.text);
            tally.cjk_chars += s.kana + s.han + s.hangul;
            tally.words += s.words;
        }
        tally
    }

    fn enough(&self) -> bool {
        self.total >= ENOUGH_MS || self.cjk_chars >= ENOUGH_CJK_CHARS || self.words >= ENOUGH_WORDS
    }

    fn leader(&self) -> Option<(MeetingLang, u64)> {
        let mut best: Option<(MeetingLang, u64)> = None;
        for &(lang, weight) in &self.weights {
            if weight > 0 && best.is_none_or(|(_, top)| weight > top) {
                best = Some((lang, weight));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Speaker;

    const JA: MeetingLang = MeetingLang::Known(Lang::Ja);
    const ZH: MeetingLang = MeetingLang::Known(Lang::Zh);
    const EN: MeetingLang = MeetingLang::Known(Lang::En);

    fn said(start_s: u64, secs: u64, text: &str) -> Utterance {
        Utterance {
            speaker: Speaker::Others,
            start_ms: start_s * 1000,
            end_ms: (start_s + secs) * 1000,
            text: text.into(),
        }
    }

    fn judge(utterances: &[Utterance], previous: Option<Lang>, prior: Option<Lang>, judge: Judge) -> LangChoice {
        let refs: Vec<&Utterance> = utterances.iter().collect();
        meeting_lang(&refs, previous, prior, judge)
    }

    fn first(utterances: &[Utterance]) -> Option<MeetingLang> {
        judge(utterances, None, None, Judge::Hint).request
    }

    #[test]
    fn each_utterance_is_classified_by_its_scripts() {
        assert_eq!(classify("配送設定は初期値をオフにします。"), Some(JA));
        assert_eq!(classify("Acme の API を v2 に上げる"), Some(JA), "English terms inside Japanese");
        assert_eq!(classify("这个の问题我们下周再看"), Some(ZH), "one kana inside Chinese");
        assert_eq!(classify("Let's ask 田中 about the budget before Friday."), Some(EN), "a Han name inside English");
        assert_eq!(classify("Nous allons lancer la version bêta la semaine prochaine."), Some(MeetingLang::Unknown));
        assert_eq!(classify("다음 주까지 예산안을 정리해서 공유하겠습니다."), Some(MeetingLang::Known(Lang::Ko)));
        assert_eq!(classify("Төсвийн төлөвлөгөөг ирэх долоо хоногт илгээнэ."), Some(MeetingLang::Unknown));
        assert_eq!(classify("12:30 … 2026"), None);
    }

    #[test]
    fn a_window_in_one_language_names_it() {
        let ja = [said(0, 20, "金曜の再テストに向けて、今日中に修正パッチを出しましょう。")];
        assert_eq!(first(&ja), Some(JA));
        let mixed = [said(0, 20, "Acme の API を v2 に上げるのは来週にして、まず検証環境で確かめます。")];
        assert_eq!(first(&mixed), Some(JA));
        let zh = [said(0, 20, "这个の问题我们下周再看，预算这块先按一百二十万报给财务。")];
        assert_eq!(first(&zh), Some(ZH));
        let en = [said(0, 20, "Let's ask 田中 about the budget, and we send it to finance before Friday.")];
        assert_eq!(first(&en), Some(EN));
        let fr = [said(0, 20, "Nous allons lancer la version bêta la semaine prochaine pour les clients.")];
        assert_eq!(first(&fr), Some(MeetingLang::Unknown));
        let ko = [said(0, 20, "다음 주까지 예산안을 정리해서 재무팀에 공유하겠습니다.")];
        assert_eq!(first(&ko), Some(MeetingLang::Known(Lang::Ko)));
        let mn = [said(0, 20, "Төсвийн төлөвлөгөөг ирэх долоо хоногт санхүүгийн хэлтэст илгээнэ.")];
        assert_eq!(first(&mn), Some(MeetingLang::Unknown));
    }

    #[test]
    fn an_unnamed_language_is_never_settled() {
        let fr = [said(0, 20, "Nous allons lancer la version bêta la semaine prochaine pour les clients.")];
        assert_eq!(judge(&fr, None, None, Judge::Hint).settled, None);
    }

    #[test]
    fn too_little_text_takes_the_prior_or_makes_no_claim() {
        let short = [said(0, 2, "はい。")];
        let none = judge(&short, None, None, Judge::Hint);
        assert_eq!((none.request, none.settled), (None, None), "no guess, and never the interface language");
        let prior = judge(&short, None, Some(Lang::Zh), Judge::Hint);
        assert_eq!(prior.request, Some(ZH));
        assert_eq!(prior.settled, None, "the prior is not what the meeting showed, so the next request decides again");
        let settled = judge(&short, Some(Lang::Ja), None, Judge::Memo);
        assert_eq!((settled.request, settled.settled), (Some(JA), Some(Lang::Ja)));
        assert_eq!(judge(&[], None, None, Judge::Memo).request, None);
    }

    #[test]
    fn speech_time_outweighs_the_number_of_utterances() {
        let mut window = vec![said(0, 30, "金曜の再テストに向けて、今日中に修正パッチを出しましょう。")];
        for i in 0..3 {
            window.push(said(30 + 2 * i, 2, "Sounds good to me, let's do that."));
        }
        assert_eq!(first(&window), Some(JA));
    }

    fn share(ja_secs: u64, en_secs: u64) -> Vec<Utterance> {
        vec![
            said(0, ja_secs, "金曜の再テストに向けて、今日中に修正パッチを出しましょう。"),
            said(ja_secs, en_secs, "We ship the patch today and the team checks it before the retest on Friday."),
        ]
    }

    #[test]
    fn a_memo_moves_a_settled_language_only_on_a_clear_majority() {
        let stays = judge(&share(35, 65), Some(Lang::Ja), None, Judge::Memo);
        assert_eq!((stays.request, stays.settled), (Some(JA), Some(Lang::Ja)));
        let moves = judge(&share(25, 75), Some(Lang::Ja), None, Judge::Memo);
        assert_eq!((moves.request, moves.settled), (Some(EN), Some(Lang::En)));
    }

    #[test]
    fn a_hint_never_moves_a_settled_language() {
        let all_english = share(0, 60);
        let hint = judge(&all_english, Some(Lang::Ja), None, Judge::Hint);
        assert_eq!((hint.request, hint.settled), (Some(JA), Some(Lang::Ja)));
    }

    #[test]
    fn a_meeting_that_moves_to_an_unnamed_language_drops_its_label() {
        let window = [
            said(0, 10, "金曜の再テストに向けて、今日中に修正パッチを出しましょう。"),
            said(10, 80, "Nous allons lancer la version bêta la semaine prochaine pour les clients."),
        ];
        let memo = judge(&window, Some(Lang::Ja), None, Judge::Memo);
        assert_eq!((memo.request, memo.settled), (Some(MeetingLang::Unknown), None));
    }

    #[test]
    fn the_sample_is_the_longest_utterance_in_the_named_language() {
        let window = [
            said(0, 5, "短い発言です。"),
            said(5, 5, "This English line is the longest one in the whole window by far."),
            said(10, 5, "配送設定は初期値をオフにします。"),
        ];
        let refs: Vec<&Utterance> = window.iter().collect();
        assert_eq!(language_sample(&refs, Some(JA)).unwrap().text, "配送設定は初期値をオフにします。");
        assert!(
            language_sample(&refs, Some(MeetingLang::Known(Lang::Ko))).is_none(),
            "never a sample in another language"
        );
        assert_eq!(language_sample(&refs, None).unwrap().start_ms, 5_000);
    }
}

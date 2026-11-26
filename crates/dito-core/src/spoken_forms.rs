//! Restores how a wordbook term is written where a transcript spells it as it sounds: `one on one`, `1 on 1` and
//! `One-on-one` become `1on1`, `c plus plus` becomes `C++`, `github` becomes `GitHub`. A hotword only hints at a term
//! and an engine may take none, so the pipeline runs this on every transcript before polishing, whatever the engine,
//! and once more on the polished text, which a style rule may spell out again.
//!
//! A rule is either derived from the shape of its term or written by the user, who may enter how a term is said
//! (`meeting` for `MTG`, or the same word in Japanese script), for every language or for one. A written rule
//! matches its form as typed, an ASCII letter in either case and a space, a hyphen or a no-break space in place of
//! another. It starts and ends at the edge of a word unless the form starts or ends with Japanese, Chinese or
//! Korean characters, which are written without spaces: such a form matches inside a sentence that has no space
//! around it, and an English form matches next to such a sentence, while a longer word stays (`meetings` is not
//! `meeting`). Of two rules that match the same span, the user's wins.
//!
//! A derived rule comes from the shape of its term, so it cannot tell the term from the same ordinary words: with
//! `R&D` in the wordbook, `r and d` is always restored. Ordinary words stay as they are because a rule is derived
//! only for:
//!
//! - a term with a digit or a symbol, unless it is only a symbol, only two characters (`2B`, `v2`) or has a hyphen at
//!   an edge. Each segment matches as written or as said, with an optional space, hyphen or no-break space between
//!   neighbors: a number below 100 is also its number words, a symbol is also the word it is said as, and a slash
//!   between two segments is also nothing at all (`ci cd`).
//! - a mixed-case word (`macOS`, `OpenAI`), as one whole token in either case. A separator is never put in or taken
//!   out for it, so `mac os` stays.
//!
//! A symbol said as a word keeps a separator on each side (`r and d`, never `rand`). A derived spelling starts and
//! ends at the edge of a word, where the letters of every script count as part of the word except Japanese, Chinese
//! and Korean characters, so a term still matches inside a sentence written without spaces around it; of several
//! that match at one place the longest is written, then one the user entered over one derived from a term, then one
//! already written as the term, then the longer term. Text that already holds a term as written is left alone, also
//! where a form the user entered is the start of that term, so restoring twice changes nothing the second time.

use std::borrow::Cow;

use crate::i18n::Lang;
use crate::store::WordbookEntry;

const ONES: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
const TEENS: [&str; 10] =
    ["ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

/// The number words of a value below 100: the tens word, then the ones word when the value has both.
type Said = (&'static str, Option<&'static str>);

/// The spoken forms of a wordbook that are written as their term, each with how.
pub struct SpokenForms {
    rules: Vec<Rule>,
}

struct Rule {
    written: String,
    pattern: Pattern,
    /// The language the rule is scoped to; `None` counts for every language.
    lang: Option<Lang>,
    /// Whether the user entered the spoken form, as opposed to it being derived from the shape of the term.
    explicit: bool,
}

enum Pattern {
    Segments(Vec<Segment>),
    /// A mixed-case word, matched as one token in either case.
    Token(String),
    /// A form the user entered for the term.
    Spoken(String),
}

enum Segment {
    Num { digits: String, said: Option<Said> },
    Word(String),
    Sym(u8),
}

impl SpokenForms {
    pub fn from_terms(terms: &[String]) -> Self {
        Self { rules: terms.iter().filter_map(|term| Rule::derive(term)).collect() }
    }

    /// One rule per spoken form the user entered, with the rule derived from the shape of each term beside them: with
    /// a Japanese form entered for `1on1`, `one on one` is still written as `1on1`.
    pub fn from_entries(entries: &[WordbookEntry]) -> Self {
        let mut rules = Vec::new();
        for entry in entries {
            rules.extend(entry.spoken.iter().filter_map(|form| Rule::entered(&entry.term, form, entry.lang)));
            rules.extend(Rule::derive(&entry.term));
        }
        Self { rules }
    }

    /// `text` with every spelling of a term written as the term, borrowed when there is none.
    pub fn apply<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.apply_for_lang(text, None)
    }

    /// [`apply`](Self::apply) on a transcript in `lang`, the only language a rule scoped to it applies to. A rule
    /// scoped to a language stays out of a transcript whose language is unknown (`None`).
    pub fn apply_for_lang<'a>(&self, text: &'a str, lang: Option<Lang>) -> Cow<'a, str> {
        if self.rules.is_empty() {
            return Cow::Borrowed(text);
        }
        // An entered form may start at a Japanese, Chinese or Korean character, or right after one, where a derived
        // rule never starts.
        let anywhere = self.rules.iter().any(|rule| matches!(rule.pattern, Pattern::Spoken(_)));
        let mut out: Option<String> = None;
        let mut copied = 0;
        // What was written is never read as a spelling again, so scanning resumes after it.
        let mut resume = 0;
        for (at, first) in text.char_indices() {
            if at < resume || !anywhere && (!first.is_ascii() || starts_inside_word(text, at)) {
                continue;
            }
            let Some((end, written)) = self.longest(text, at, lang) else {
                continue;
            };
            if &text[at..end] != written {
                let out = out.get_or_insert_with(|| String::with_capacity(text.len()));
                out.push_str(&text[copied..at]);
                out.push_str(written);
                copied = end;
            }
            resume = end;
        }
        match out {
            Some(mut out) => {
                out.push_str(&text[copied..]);
                Cow::Owned(out)
            }
            None => Cow::Borrowed(text),
        }
    }

    /// What is written at `at`: the longest, then one the user entered over one derived from a term, then one
    /// already written as its term, then the longer term.
    fn longest<'a>(&'a self, text: &str, at: usize, lang: Option<Lang>) -> Option<(usize, &'a str)> {
        let before = text[..at].chars().next_back();
        let (end, written, _) = self
            .rules
            .iter()
            .rev()
            .filter(|rule| rule.lang.is_none_or(|scoped| Some(scoped) == lang))
            .flat_map(|rule| rule.ends(text, at, before))
            .max_by_key(|&(end, written, explicit)| (end - at, explicit, &text[at..end] == written, written.len()))?;
        Some((end, written))
    }
}

/// `text` with every spelling of a wordbook term written as the term.
pub fn normalize<'a>(text: &'a str, terms: &[String]) -> Cow<'a, str> {
    SpokenForms::from_terms(terms).apply(text)
}

/// `text` with every spoken form of the wordbook entries written as its term, on a transcript in `lang`.
pub fn normalize_entries<'a>(text: &'a str, entries: &[WordbookEntry], lang: Option<Lang>) -> Cow<'a, str> {
    SpokenForms::from_entries(entries).apply_for_lang(text, lang)
}

impl Rule {
    /// The rule the shape of `term` gives, for a term that has one.
    fn derive(term: &str) -> Option<Self> {
        let term = term.trim();
        Some(Self { written: term.to_owned(), pattern: derived(term)?, lang: None, explicit: false })
    }

    /// The rule of the spoken form `form` the user entered for `term`; `None` for a blank form.
    fn entered(term: &str, form: &str, lang: Option<Lang>) -> Option<Self> {
        let form = form.trim();
        (!form.is_empty()).then(|| Self {
            written: term.to_owned(),
            pattern: Pattern::Spoken(form.to_owned()),
            lang,
            explicit: true,
        })
    }

    /// Where a spelling of the term ends at `at` and where the term as already written ends: each end with the
    /// term to write and whether the user entered the rule. `before` is the character ahead of `at`.
    fn ends<'a>(&'a self, text: &str, at: usize, before: Option<char>) -> impl Iterator<Item = (usize, &'a str, bool)> {
        let written = self.written.as_str();
        self.matching(text, at, before)
            .into_iter()
            .chain(self.already_written(text, at, before))
            .map(move |end| (end, written, self.explicit))
    }

    /// The farthest end of a spelling of the term that starts at `at` and ends a word, if there is one.
    fn matching(&self, text: &str, at: usize, before: Option<char>) -> Option<usize> {
        if self.starts_at_word_edge() && before.is_some_and(is_word_char) {
            return None;
        }
        match &self.pattern {
            Pattern::Spoken(phrase) => {
                let end = spoken_end(text, at, phrase)?;
                (phrase.ends_with(|c| !word_edge(c)) || ends_word(text, end)).then_some(end)
            }
            Pattern::Token(token) => word_end(text.as_bytes(), at, token).filter(|&end| ends_word(text, end)),
            Pattern::Segments(segments) => spelled_ends(text, at, segments)
                .into_iter()
                .map(|(end, _)| end)
                .filter(|&end| ends_word(text, end))
                .max(),
        }
    }

    /// The end of the term at `at`, when the text already holds it between the edges of a word. A form the user
    /// entered may be the start of its own term, which would else be written into the term again on the next pass.
    fn already_written(&self, text: &str, at: usize, before: Option<char>) -> Option<usize> {
        let written = self.written.as_str();
        if written.is_empty() || !text[at..].starts_with(written) {
            return None;
        }
        let end = at + written.len();
        let inside = (written.starts_with(is_word_char) && before.is_some_and(is_word_char))
            || (written.ends_with(is_word_char) && !ends_word(text, end));
        (!inside).then_some(end)
    }

    /// Whether a spelling of the rule must start at the edge of a word. One that begins with Japanese, Chinese or
    /// Korean characters need not, as they are written without spaces.
    fn starts_at_word_edge(&self) -> bool {
        match &self.pattern {
            Pattern::Spoken(phrase) => phrase.starts_with(word_edge),
            _ => true,
        }
    }
}

/// The pattern of the rule the shape of `term` gives, for a term that has one.
fn derived(term: &str) -> Option<Pattern> {
    // A hyphen only separates inside a term. One at an edge is part of no spelling, so writing the term would
    // add it again on every pass.
    if term.starts_with('-') || term.ends_with('-') {
        return None;
    }
    if !term.is_ascii() {
        return None;
    }
    let bytes = term.as_bytes();
    if bytes.iter().any(|byte| byte.is_ascii_digit() || is_symbol(*byte)) {
        let segments = split(term)?;
        let alnum = segments.iter().filter(|segment| !matches!(segment, Segment::Sym(_))).count();
        let symbol = segments.iter().any(|segment| matches!(segment, Segment::Sym(_)));
        let short = bytes.iter().filter(|byte| byte.is_ascii_alphanumeric()).count() <= 2;
        // A lone symbol would turn every `and` into `&`, and `4K` is too short to tell from what it sounds like.
        if alnum == 0 || (!symbol && (alnum < 2 || short)) {
            return None;
        }
        return Some(Pattern::Segments(segments));
    }
    let one_word = bytes.iter().all(|byte| byte.is_ascii_alphabetic());
    let mixed_case = bytes.iter().skip(1).any(|byte| byte.is_ascii_uppercase())
        && bytes.iter().any(|byte| byte.is_ascii_lowercase());
    (one_word && mixed_case).then(|| Pattern::Token(term.to_ascii_lowercase()))
}

#[derive(Clone, Copy)]
enum Kind {
    Digit,
    Lower,
    Upper,
    Space,
    Symbol,
}

impl Kind {
    fn of(byte: u8) -> Option<Self> {
        match byte {
            b'0'..=b'9' => Some(Self::Digit),
            b'a'..=b'z' => Some(Self::Lower),
            b'A'..=b'Z' => Some(Self::Upper),
            b' ' | b'-' => Some(Self::Space),
            byte if is_symbol(byte) => Some(Self::Symbol),
            _ => None,
        }
    }

    /// Whether `next` continues the run of this kind.
    fn joins(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Digit, Self::Digit) | (Self::Lower, Self::Lower) | (Self::Upper, Self::Upper | Self::Lower)
        )
    }
}

/// The letters, digits and symbols of a term: a new segment where a digit meets a letter, a lowercase letter meets an
/// uppercase one, and at every symbol. A space or a hyphen only separates. `None` for any other character.
fn split(term: &str) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut run = String::new();
    let mut kind: Option<Kind> = None;
    for byte in term.bytes() {
        let next = Kind::of(byte)?;
        if kind.is_some_and(|previous| !previous.joins(next)) {
            close_run(&mut segments, kind, &mut run);
        }
        match next {
            Kind::Digit | Kind::Lower | Kind::Upper => run.push(byte.to_ascii_lowercase() as char),
            Kind::Space => {}
            Kind::Symbol => segments.push(Segment::Sym(byte)),
        }
        kind = Some(next);
    }
    close_run(&mut segments, kind, &mut run);
    Some(segments)
}

fn close_run(segments: &mut Vec<Segment>, kind: Option<Kind>, run: &mut String) {
    match kind {
        Some(Kind::Digit) => segments.push(number(std::mem::take(run))),
        Some(Kind::Upper | Kind::Lower) => segments.push(Segment::Word(std::mem::take(run))),
        _ => {}
    }
}

/// The digits, with their number words when they are a plain value: `24` is said `twenty four`, `007` and `100` are
/// not said at all.
fn number(digits: String) -> Segment {
    let plain = digits == "0" || (digits.len() <= 2 && !digits.starts_with('0'));
    let said = digits.parse::<u8>().ok().filter(|_| plain).map(number_words);
    Segment::Num { digits, said }
}

fn number_words(value: u8) -> Said {
    match value {
        0..=9 => (ONES[value as usize], None),
        10..=19 => (TEENS[value as usize - 10], None),
        20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 => (TENS[value as usize / 10], None),
        _ => (TENS[value as usize / 10], Some(ONES[value as usize % 10])),
    }
}

/// Every position where a spelling of `segments` from `at` can end, each with whether a symbol word ends there. A
/// segment is matched from every position the ones before it reached, so a term with many slashes costs one pass per
/// segment rather than a path per way of reading them.
fn spelled_ends(text: &str, at: usize, segments: &[Segment]) -> Vec<(usize, bool)> {
    let bytes = text.as_bytes();
    let Some((first, rest)) = segments.split_first() else {
        return Vec::new();
    };
    let Some(first) = (match first {
        Segment::Num { digits, said } => number_end(bytes, at, digits, *said).map(|end| (end, false)),
        Segment::Word(word) => word_end(bytes, at, word).map(|end| (end, false)),
        Segment::Sym(symbol) => symbol_end(text, at, *symbol, false, true),
    }) else {
        return Vec::new();
    };
    let mut states = vec![first];
    let mut reached = Vec::new();
    for (index, segment) in rest.iter().enumerate() {
        reached.clear();
        for &(pos, after_word) in &states {
            match segment {
                Segment::Num { digits, said } => {
                    if let Some(start) = next_start(bytes, pos, after_word)
                        && let Some(end) = number_end(bytes, start, digits, *said)
                    {
                        reached.push((end, false));
                    }
                }
                Segment::Word(word) => {
                    if let Some(start) = next_start(bytes, pos, after_word)
                        && let Some(end) = word_end(bytes, start, word)
                    {
                        reached.push((end, false));
                    }
                }
                Segment::Sym(symbol) => {
                    // Right after the previous segment only the symbol itself, or a slash left out: a symbol word
                    // always has a separator in front, so `rand` never spells `R&D`.
                    if !after_word && let Some(found) = symbol_end(text, pos, *symbol, index + 1 < rest.len(), false) {
                        reached.push(found);
                    }
                    if let Some(after) = separator_end(bytes, pos)
                        && let Some(found) = symbol_end(text, after, *symbol, false, true)
                    {
                        reached.push(found);
                    }
                }
            }
        }
        reached.sort_unstable();
        reached.dedup();
        std::mem::swap(&mut states, &mut reached);
    }
    states
}

/// Where the next segment starts: right after the previous one or after one separator, and only after one separator
/// when the previous one is a symbol word.
fn next_start(bytes: &[u8], at: usize, after_word: bool) -> Option<usize> {
    match separator_end(bytes, at) {
        Some(after) => Some(after),
        None if after_word => None,
        None => Some(at),
    }
}

/// The end of the digits at `at`, written or said.
fn number_end(bytes: &[u8], at: usize, digits: &str, said: Option<Said>) -> Option<usize> {
    let end = at + digits.len();
    if bytes.get(at..end)? == digits.as_bytes() {
        return Some(end);
    }
    let (tens, ones) = said?;
    let end = word_end(bytes, at, tens)?;
    match ones {
        None => Some(end),
        Some(ones) => word_end(bytes, separator_end(bytes, end)?, ones),
    }
}

/// The end of `word` at `at`, in either case.
fn word_end(bytes: &[u8], at: usize, word: &str) -> Option<usize> {
    let end = at + word.len();
    bytes.get(at..end)?.eq_ignore_ascii_case(word.as_bytes()).then_some(end)
}

/// The end of the symbol at `at` and whether it was said as a word: the symbol itself, a separator when `bare`, or
/// the word it is said as when `word`.
fn symbol_end(text: &str, at: usize, symbol: u8, bare: bool, word: bool) -> Option<(usize, bool)> {
    let bytes = text.as_bytes();
    if bytes.get(at) == Some(&symbol) {
        return Some((at + 1, false));
    }
    // A slash between two segments is also left out: `ci cd` is `CI/CD`.
    if bare && symbol == b'/' {
        return separator_end(bytes, at).map(|end| (end, false));
    }
    let end = word_end(bytes, at, symbol_word(symbol)?)?;
    // The word is whole: `andy` does not say `&`.
    let whole = word && text[end..].chars().next().is_none_or(|next| !is_word_char(next));
    whole.then_some((end, true))
}

/// The word a symbol is said as.
fn symbol_word(symbol: u8) -> Option<&'static str> {
    match symbol {
        b'+' => Some("plus"),
        b'#' => Some("sharp"),
        b'.' => Some("dot"),
        b'&' => Some("and"),
        b'/' => Some("slash"),
        _ => None,
    }
}

/// The end of the separator at `at`: a space, a hyphen or a no-break space.
fn separator_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes.get(at) {
        Some(b' ' | b'-') => Some(at + 1),
        Some(0xC2) if bytes.get(at + 1) == Some(&0xA0) => Some(at + 2),
        _ => None,
    }
}

/// The end of `phrase`, a form the user entered, at `at`: a space, a hyphen or a no-break space of the phrase matches
/// any of the three, an ASCII character matches in either case, and any other character only itself.
fn spoken_end(text: &str, at: usize, phrase: &str) -> Option<usize> {
    let mut end = at;
    for want in phrase.chars() {
        let found = text[end..].chars().next()?;
        let same = match want {
            ' ' | '-' | '\u{00A0}' => matches!(found, ' ' | '-' | '\u{00A0}'),
            _ if want.is_ascii() => found.eq_ignore_ascii_case(&want),
            _ => want == found,
        };
        if !same {
            return None;
        }
        end += found.len_utf8();
    }
    Some(end)
}

/// Whether a spelling that begins or ends with `c` must sit at the edge of a word. Japanese, Chinese and Korean
/// characters are written without spaces, so an entered form made of them matches next to any character; every other
/// character is part of a word, a symbol included.
fn word_edge(c: char) -> bool {
    c.is_ascii() || is_word_char(c)
}

/// Whether a letter or digit comes right before `at`, where no spelling may start.
fn starts_inside_word(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_some_and(is_word_char)
}

/// Whether a spelling that ends at `end` ends a word: no letter or digit follows, and a symbol is not the start of a
/// longer run of itself (`c++x` and `c+++` are not `C++`).
fn ends_word(text: &str, end: usize) -> bool {
    let Some(next) = text[end..].chars().next() else {
        return true;
    };
    let last = text.as_bytes()[end - 1];
    !is_word_char(next) && !(next.is_ascii() && next as u8 == last && is_symbol(last))
}

/// Whether `c` is part of a word, so that a term is not matched inside one: any letter or digit, also one with an
/// accent, and a combining mark. Japanese, Chinese and Korean characters are not, so a term still matches next to
/// them without a space.
fn is_word_char(c: char) -> bool {
    let unspaced = matches!(
        c,
        '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{FF66}'..='\u{FF9F}'
    );
    !unspaced && (c.is_alphanumeric() || matches!(c, '\u{0300}'..='\u{036F}'))
}

fn is_symbol(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'/' | b'.' | b'&')
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::i18n::Lang;
    use crate::store::WordbookEntry;

    fn terms(terms: &[&str]) -> Vec<String> {
        terms.iter().map(|term| term.to_string()).collect()
    }

    fn entries(list: &[(&str, &[&str], Option<Lang>)]) -> Vec<WordbookEntry> {
        list.iter()
            .map(|(term, spoken, lang)| WordbookEntry {
                term: term.to_string(),
                spoken: spoken.iter().map(|form| form.to_string()).collect(),
                lang: *lang,
            })
            .collect()
    }

    fn said(text: &str, wordbook: &[WordbookEntry], lang: Option<Lang>) -> String {
        normalize_entries(text, wordbook, lang).into_owned()
    }

    fn stays(text: &str, term: &str) {
        assert!(matches!(normalize(text, &terms(&[term])), Cow::Borrowed(_)), "{text} with {term}");
    }

    fn restores(term: &str, spelled: &str, written: &str) {
        let wordbook = terms(&[term]);
        let once = normalize(spelled, &wordbook);
        assert_eq!(once, written, "{spelled} with {term}");
        assert_eq!(normalize(&once, &wordbook), written, "{written} with {term}, restored again");
        assert!(matches!(normalize(written, &wordbook), Cow::Borrowed(_)), "{written} with {term} is already written");
    }

    #[test]
    fn a_term_is_written_as_itself_however_it_was_spelled() {
        for (term, spelled, written) in [
            ("1on1", "one on one", "1on1"),
            ("1on1", "One-on-one", "1on1"),
            ("1on1", "1 on 1", "1on1"),
            ("1on1", "1-on-1", "1on1"),
            ("1on1", "oneonone", "1on1"),
            ("1on1", "one\u{00a0}on\u{00a0}one", "1on1"),
            ("1on1", "今日は one on one です", "今日は 1on1 です"),
            ("1on1", "明天有个 one on one 会议", "明天有个 1on1 会议"),
            ("1on1", "はone on oneです", "は1on1です"),
            ("1on1", "we did one on one and 1 on 1", "we did 1on1 and 1on1"),
            ("C++", "use c plus plus here", "use C++ here"),
            ("C++", "use c++ here", "use C++ here"),
            ("C#", "c sharp", "C#"),
            ("CI/CD", "ci/cd", "CI/CD"),
            ("CI/CD", "ci-cd", "CI/CD"),
            ("CI/CD", "ci / cd", "CI/CD"),
            ("CI/CD", "ci slash cd", "CI/CD"),
            ("A///B", "a / / / b", "A///B"),
            ("A//", "a //", "A//"),
            ("N/A", "n a", "N/A"),
            ("24/7", "24 7", "24/7"),
            ("24/7", "24 seven", "24/7"),
            ("24/7", "twenty four seven", "24/7"),
            ("24/7", "twenty-four seven", "24/7"),
            ("24/7", "twenty four slash seven", "24/7"),
            ("GPT-4", "gpt four", "GPT-4"),
            ("Node.js", "node dot js", "Node.js"),
            ("R&D", "r and d", "R&D"),
            (".NET", "use .net here", "use .NET here"),
            (".NET", " dot net", " .NET"),
            ("GitHub", "github", "GitHub"),
            ("GitHub", "github.com", "GitHub.com"),
            ("macOS", "MACOS", "macOS"),
            ("iPhone15", "iphone fifteen", "iPhone15"),
            ("G21", "g twenty-one", "G21"),
            ("G21", "g twenty one", "G21"),
            ("MP3", "mp three", "MP3"),
            ("B2B", "b-two-b", "B2B"),
        ] {
            restores(term, spelled, written);
        }
    }

    #[test]
    fn a_term_with_many_slashes_is_matched_in_one_pass_per_segment() {
        let slashes = "/".repeat(24);
        restores(&format!("A{slashes}B"), &format!("a{} b", " /".repeat(24)), &format!("A{slashes}B"));
    }

    #[test]
    fn what_is_not_a_term_spelled_out_stays_as_it_is() {
        for (term, text) in [
            ("1on1", "someone on one"),
            ("1on1", "one on ones"),
            ("1on1", "one on 11"),
            ("1on1", "11 on one"),
            ("1on1", "won on won"),
            ("24/7", "for seven"),
            ("OpenAI", "open AI tools"),
            ("macOS", "mac os"),
            ("Node.js", "node js"),
            ("Node.js", "nodedotjs"),
            ("CI/CD", "cicd"),
            ("C++", "c++x"),
            ("C++", "c+++"),
            ("C#", "c#5"),
            ("24/7", "twenty four"),
            ("L&D", "the land deal"),
            ("R&D", "the rand crate"),
            ("w/", "w you"),
            ("/tmp", "see tmp later"),
        ] {
            stays(text, term);
        }
    }

    #[test]
    fn a_term_is_not_matched_inside_a_word_of_any_script() {
        for (term, text) in [
            ("GitHub", "prégithub"),
            ("GitHub", "cafe\u{0301}github"),
            ("GitHub", "αgithub"),
            ("CaF", "cafés"),
            ("1on1", "éone on one"),
            ("1on1", "one on oneé"),
            ("R&D", "r and dé"),
            ("R&D", "r andé d"),
        ] {
            stays(text, term);
        }
    }

    #[test]
    fn a_term_with_a_hyphen_at_an_edge_gets_no_rule() {
        for (term, text) in [
            ("-O2", "-O2"),
            ("-O2", "use -O2 now"),
            ("-O2", "O2"),
            (" -O2 ", "-O2"),
            ("1on1-", "one on one"),
            ("1on1-", "1on1"),
        ] {
            stays(text, term);
        }
    }

    #[test]
    fn a_term_spelled_like_ordinary_words_gets_no_rule() {
        for (term, text) in [
            ("plain", "plain word"),
            ("Visual Studio", "visual studio"),
            ("Visual Studio", "Visual-Studio"),
            ("Claude", "claude"),
            ("AI", "ai"),
            ("IOS", "ios"),
            ("2B", "to be"),
            ("2B", "two b"),
            ("4K", "four k"),
            ("5G", "five g"),
            ("v2", "v two"),
            ("100", "one hundred"),
            ("007", "zero zero seven"),
            ("21", "twenty one"),
            ("東京2020", "東京 2020"),
            ("サンプル株式会社", "サンプル株式会社"),
            ("McDonald's", "mcdonalds"),
            ("foo_bar", "foo bar"),
            ("&", "and"),
            ("+", "plus"),
            ("/", "a b c"),
            (".", "dot"),
            ("#", "sharp"),
        ] {
            stays(text, term);
        }
        assert!(matches!(normalize("one on one", &[]), Cow::Borrowed(_)));
    }

    #[test]
    fn a_mixed_case_word_is_one_whole_token() {
        let wordbook = terms(&["GitHub", "macOS", "OpenAI"]);
        assert_eq!(
            normalize("I use github, MacOS and OPENAI daily", &wordbook),
            "I use GitHub, macOS and OpenAI daily"
        );
        for text in ["open AI tools", "mygithub", "githubbed"] {
            assert!(matches!(normalize(text, &wordbook), Cow::Borrowed(_)), "{text}");
        }
    }

    #[test]
    fn the_longest_spelling_wins_and_one_already_written_is_kept() {
        assert_eq!(normalize("gpt four o", &terms(&["GPT4", "GPT4o"])), "GPT4o");
        assert_eq!(normalize("gpt four", &terms(&["GPT4", "GPT4o"])), "GPT4");
        assert_eq!(normalize("one on one", &terms(&["1on1", "1-on-1"])), "1-on-1");
        assert_eq!(normalize("one on one", &terms(&["1-on-1", "1on1"])), "1-on-1");
        let wordbook = terms(&["GPT4", "gpt4"]);
        for text in ["GPT4 and gpt4", "gpt4 and GPT4"] {
            assert!(matches!(normalize(text, &wordbook), Cow::Borrowed(_)), "{text}");
        }
        let once = normalize("GPT four and gpt four", &wordbook);
        assert_eq!(normalize(&once, &wordbook), once);
    }

    #[test]
    fn an_entered_form_is_written_as_its_term_in_its_language_and_at_word_edges() {
        let wordbook = entries(&[("MTG", &["meeting", "ミーティング"], Some(Lang::Ja))]);
        for (heard, written) in [
            ("明日のミーティング", "明日のMTG"),
            ("ミーティングです", "MTGです"),
            ("はミーティングです", "はMTGです"),
            ("明日のmeetingで", "明日のMTGで"),
            ("meeting の予定", "MTG の予定"),
            ("the Meeting is at three", "the MTG is at three"),
        ] {
            let once = said(heard, &wordbook, Some(Lang::Ja));
            assert_eq!(once, written, "{heard}");
            assert_eq!(said(&once, &wordbook, Some(Lang::Ja)), written, "{written}, restored again");
        }
        for text in ["meetings", "planning", "MTG"] {
            assert_eq!(said(text, &wordbook, Some(Lang::Ja)), text, "{text} in Japanese");
        }
        for lang in [Some(Lang::En), Some(Lang::Zh), None] {
            let english = "We have a meeting tomorrow";
            assert_eq!(said(english, &wordbook, lang), english, "in {lang:?}");
            assert_eq!(said("明日のミーティング", &wordbook, lang), "明日のミーティング", "in {lang:?}");
        }
    }

    #[test]
    fn an_entered_form_works_beside_the_one_derived_from_the_term() {
        let wordbook = entries(&[("1on1", &["ワンオンワン"], None)]);
        for (heard, written) in [
            ("明日のワンオンワン", "明日の1on1"),
            ("明日の one on one", "明日の 1on1"),
            ("let's do a one-on-one", "let's do a 1on1"),
        ] {
            assert_eq!(said(heard, &wordbook, Some(Lang::Ja)), written, "{heard}");
            assert_eq!(said(heard, &wordbook, Some(Lang::En)), written, "{heard}, unscoped");
        }
    }

    #[test]
    fn an_entered_form_matches_in_either_case_with_any_separator() {
        let wordbook = entries(&[("1:1", &["One on one"], None)]);
        for (heard, written) in [
            ("one on one", "1:1"),
            ("ONE ON ONE", "1:1"),
            ("one-on-one", "1:1"),
            ("one\u{00a0}on\u{00a0}one", "1:1"),
            ("an one on oner", "an one on oner"),
            ("someone on one", "someone on one"),
        ] {
            assert_eq!(said(heard, &wordbook, None), written, "{heard}");
        }
    }

    #[test]
    fn an_entered_form_wins_over_a_derived_rule_only_on_the_same_span() {
        let wordbook =
            entries(&[("GitHub", &[], None), ("Git Hub", &["github"], None), ("Git", &["github page"], None)]);
        let once = said("on github today", &wordbook, None);
        assert_eq!(once, "on Git Hub today");
        assert_eq!(said(&once, &wordbook, None), once, "restored again");
        assert_eq!(said("see the github page", &wordbook, None), "see the Git", "the longest spelling wins");
    }

    #[test]
    fn a_term_is_not_written_into_itself_when_one_of_its_forms_is_its_start() {
        let wordbook = entries(&[("Acme Labs", &["Acme"], None)]);
        let once = said("Acme", &wordbook, None);
        assert_eq!(once, "Acme Labs");
        assert_eq!(said(&once, &wordbook, None), "Acme Labs", "restored again");
        assert_eq!(said("Acme Labs Inc and Acme", &wordbook, None), "Acme Labs Inc and Acme Labs");
        assert!(matches!(normalize_entries("AcmeLaboratories", &wordbook, None), Cow::Borrowed(_)));
    }

    #[test]
    fn an_entered_rule_wins_over_a_derived_one_whatever_the_case() {
        let wordbook = entries(&[("GitHub", &[], None), ("GH", &["github"], None)]);
        assert_eq!(said("github and GitHub", &wordbook, None), "GH and GH");
        assert!(matches!(normalize_entries("GH", &wordbook, None), Cow::Borrowed(_)), "already written as its term");
    }

    #[test]
    fn text_already_written_as_the_term_is_left_alone_next_to_an_entered_form() {
        let wordbook = entries(&[("MTG", &["mtg"], None), ("Acme", &[""], None)]);
        assert!(matches!(normalize_entries("MTG", &wordbook, None), Cow::Borrowed(_)));
        assert_eq!(said("Mtg", &wordbook, None), "MTG");
        assert!(matches!(normalize_entries("Acme", &wordbook, None), Cow::Borrowed(_)), "a blank form gives no rule");
    }
}

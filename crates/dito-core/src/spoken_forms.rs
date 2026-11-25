//! Restores how a wordbook term is written where a transcript spells it as it sounds: `one on one`, `1 on 1` and
//! `One-on-one` become `1on1`, `c plus plus` becomes `C++`, `github` becomes `GitHub`. A hotword only hints at a term
//! and an engine may take none, so the pipeline runs this on every transcript before polishing, whatever the engine,
//! and once more on the polished text, which a style rule may spell out again.
//!
//! A rule comes from the shape of its term, so it cannot tell the term from the same ordinary words: with `R&D` in the
//! wordbook, `r and d` is always restored. Ordinary words stay as they are because a rule is derived only for:
//!
//! - a term with a digit or a symbol, unless it is only a symbol or only two characters (`2B`, `v2`). Each segment
//!   matches as written or as said, with an optional space, hyphen or no-break space between neighbors: a number below
//!   100 is also its number words, a symbol is also the word it is said as, and a slash between two segments is also
//!   nothing at all (`ci cd`).
//! - a mixed-case word (`macOS`, `OpenAI`), as one whole token in either case. A separator is never put in or taken
//!   out for it, so `mac os` stays.
//!
//! A symbol said as a word keeps a separator on each side (`r and d`, never `rand`), a match starts and ends at the
//! edge of a word, and text already written as the term is left alone, so restoring twice changes nothing the second
//! time.

use std::borrow::Cow;

const ONES: [&str; 10] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
const TEENS: [&str; 10] =
    ["ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

/// The number words of a value below 100: the tens word, then the ones word when the value has both.
type Said = (&'static str, Option<&'static str>);

/// The terms of a wordbook that can be spelled as they sound, each with how.
pub struct SpokenForms {
    rules: Vec<Rule>,
}

struct Rule {
    written: String,
    pattern: Pattern,
}

enum Pattern {
    Segments(Vec<Segment>),
    /// A mixed-case word, matched as one token in either case.
    Token(String),
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

    /// `text` with every spelling of a term written as the term, borrowed when there is none.
    pub fn apply<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.rules.is_empty() {
            return Cow::Borrowed(text);
        }
        let bytes = text.as_bytes();
        let mut out: Option<String> = None;
        let mut copied = 0;
        let mut at = 0;
        while at < bytes.len() {
            let word_start = bytes[at].is_ascii() && (at == 0 || !bytes[at - 1].is_ascii_alphanumeric());
            let Some((end, written)) = word_start.then(|| self.longest(bytes, at)).flatten() else {
                at += 1;
                continue;
            };
            if &text[at..end] != written {
                let out = out.get_or_insert_with(|| String::with_capacity(text.len()));
                out.push_str(&text[copied..at]);
                out.push_str(written);
                copied = end;
            }
            // What was written is never read as a spelling again.
            at = end;
        }
        match out {
            Some(mut out) => {
                out.push_str(&text[copied..]);
                Cow::Owned(out)
            }
            None => Cow::Borrowed(text),
        }
    }

    /// The longest spelling of a term at `at`; between two of the same length, the longer term.
    fn longest<'a>(&'a self, bytes: &[u8], at: usize) -> Option<(usize, &'a str)> {
        self.rules
            .iter()
            .filter_map(|rule| Some((rule.matching(bytes, at)?, rule.written.as_str())))
            .max_by_key(|(end, written)| (*end - at, written.len()))
    }
}

/// `text` with every spelling of a wordbook term written as the term.
pub fn normalize<'a>(text: &'a str, terms: &[String]) -> Cow<'a, str> {
    SpokenForms::from_terms(terms).apply(text)
}

impl Rule {
    fn derive(term: &str) -> Option<Self> {
        let term = term.trim();
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
            return Some(Self { written: term.to_owned(), pattern: Pattern::Segments(segments) });
        }
        let one_word = bytes.iter().all(|byte| byte.is_ascii_alphabetic());
        let mixed_case = bytes.iter().skip(1).any(|byte| byte.is_ascii_uppercase())
            && bytes.iter().any(|byte| byte.is_ascii_lowercase());
        (one_word && mixed_case)
            .then(|| Self { written: term.to_owned(), pattern: Pattern::Token(term.to_ascii_lowercase()) })
    }

    /// Where a spelling of the term that starts at `at` ends, when `at` is the start of a word and the spelling ends
    /// at the end of one.
    fn matching(&self, bytes: &[u8], at: usize) -> Option<usize> {
        let end = match &self.pattern {
            Pattern::Token(token) => word_end(bytes, at, token)?,
            Pattern::Segments(segments) => spelled_end(bytes, at, segments)?,
        };
        let Some(&next) = bytes.get(end) else {
            return Some(end);
        };
        // `c++x` is not `C++`, and neither is the start of a longer run of the same symbol (`c+++`).
        let run = is_symbol(next) && bytes[end - 1] == next;
        (!next.is_ascii_alphanumeric() && !run).then_some(end)
    }
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

/// Where the spelling of `segments` that starts exactly at `at` ends.
fn spelled_end(bytes: &[u8], at: usize, segments: &[Segment]) -> Option<usize> {
    let (first, rest) = segments.split_first()?;
    let (end, as_word) = match first {
        Segment::Num { digits, said } => (number_end(bytes, at, digits, *said)?, false),
        Segment::Word(word) => (word_end(bytes, at, word)?, false),
        Segment::Sym(symbol) => symbol_end(bytes, at, *symbol, false, true)?,
    };
    segments_end(bytes, end, rest, as_word)
}

/// Where `segments`, each after a segment before it, end. `after_word` says a symbol word comes right before, which
/// needs a separator after it.
fn segments_end(bytes: &[u8], at: usize, segments: &[Segment], after_word: bool) -> Option<usize> {
    let Some((segment, rest)) = segments.split_first() else {
        return Some(at);
    };
    let rest_end = |end: usize, as_word: bool| segments_end(bytes, end, rest, as_word);
    match segment {
        Segment::Sym(symbol) => {
            // Right after the previous segment only the symbol itself, or a slash left out: a symbol word always
            // has a separator in front, so `rand` never spells `R&D`. What follows decides between this and the
            // symbol after a separator: in `ci / cd` the slash is the one after the separator.
            if !after_word
                && let Some((end, as_word)) = symbol_end(bytes, at, *symbol, !rest.is_empty(), false)
                && let Some(last) = rest_end(end, as_word)
            {
                return Some(last);
            }
            let (end, as_word) = symbol_end(bytes, separator_end(bytes, at)?, *symbol, false, true)?;
            rest_end(end, as_word)
        }
        Segment::Num { digits, said } => {
            let end = number_end(bytes, next_start(bytes, at, after_word)?, digits, *said)?;
            rest_end(end, false)
        }
        Segment::Word(word) => {
            let end = word_end(bytes, next_start(bytes, at, after_word)?, word)?;
            rest_end(end, false)
        }
    }
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
fn symbol_end(bytes: &[u8], at: usize, symbol: u8, bare: bool, word: bool) -> Option<(usize, bool)> {
    if bytes.get(at) == Some(&symbol) {
        return Some((at + 1, false));
    }
    // A slash between two segments is also left out: `ci cd` is `CI/CD`.
    if bare && symbol == b'/' {
        return separator_end(bytes, at).map(|end| (end, false));
    }
    let end = word_end(bytes, at, symbol_word(symbol)?)?;
    // The word is whole: `andy` does not say `&`.
    let whole = word && bytes.get(end).is_none_or(|next| !next.is_ascii_alphanumeric());
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

fn is_symbol(byte: u8) -> bool {
    matches!(byte, b'+' | b'#' | b'/' | b'.' | b'&')
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn terms(terms: &[&str]) -> Vec<String> {
        terms.iter().map(|term| term.to_string()).collect()
    }

    #[test]
    fn a_term_is_written_as_itself_however_it_was_spelled() {
        let cases = [
            ("1on1", "one on one", "1on1"),
            ("1on1", "One-on-one", "1on1"),
            ("1on1", "1 on 1", "1on1"),
            ("1on1", "1-on-1", "1on1"),
            ("1on1", "oneonone", "1on1"),
            ("1on1", "one\u{00a0}on\u{00a0}one", "1on1"),
            ("1on1", "今日は one on one です", "今日は 1on1 です"),
            ("1on1", "明天有个 one on one 会议", "明天有个 1on1 会议"),
            ("1on1", "we did one on one and 1 on 1", "we did 1on1 and 1on1"),
            ("C++", "use c plus plus here", "use C++ here"),
            ("C++", "use c++ here", "use C++ here"),
            ("C#", "c sharp", "C#"),
            ("CI/CD", "ci/cd", "CI/CD"),
            ("CI/CD", "ci-cd", "CI/CD"),
            ("CI/CD", "ci / cd", "CI/CD"),
            ("CI/CD", "ci slash cd", "CI/CD"),
            ("N/A", "n a", "N/A"),
            ("24/7", "24 7", "24/7"),
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
            ("24/7", "24 seven", "24/7"),
            ("MP3", "mp three", "MP3"),
            ("B2B", "b-two-b", "B2B"),
        ];
        for (term, spelled, written) in cases {
            let wordbook = terms(&[term]);
            let once = normalize(spelled, &wordbook);
            assert_eq!(once, written, "{spelled}");
            assert_eq!(normalize(&once, &wordbook), written, "{written} restored again");
            assert!(matches!(normalize(written, &wordbook), Cow::Borrowed(_)), "{written} is already written");
        }
    }

    #[test]
    fn what_is_not_a_term_spelled_out_stays_as_it_is() {
        let cases = [
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
        ];
        for (term, text) in cases {
            let wordbook = terms(&[term]);
            assert!(matches!(normalize(text, &wordbook), Cow::Borrowed(_)), "{text} with {term}");
        }
    }

    #[test]
    fn a_term_spelled_like_ordinary_words_gets_no_rule() {
        let cases = [
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
        ];
        for (term, text) in cases {
            let wordbook = terms(&[term]);
            assert!(matches!(normalize(text, &wordbook), Cow::Borrowed(_)), "{text} with {term}");
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
    fn the_longest_spelling_wins_and_between_equal_ones_the_longer_term() {
        assert_eq!(normalize("gpt four o", &terms(&["GPT4", "GPT4o"])), "GPT4o");
        assert_eq!(normalize("gpt four", &terms(&["GPT4", "GPT4o"])), "GPT4");
        assert_eq!(normalize("one on one", &terms(&["1on1", "1-on-1"])), "1-on-1");
        assert_eq!(normalize("one on one", &terms(&["1-on-1", "1on1"])), "1-on-1");
    }
}

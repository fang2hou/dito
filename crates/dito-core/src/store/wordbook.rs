//! SQLite-backed wordbook.

use std::collections::HashMap;

use rusqlite::{Connection, Statement, params};

use super::*;
use crate::i18n::Lang;

/// One term of the wordbook with the phrases speech recognition may hear instead of it, and the language they are
/// listened for in (`None`: every language).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordbookEntry {
    pub term: String,
    pub spoken: Vec<String>,
    pub lang: Option<Lang>,
}

impl WordbookEntry {
    /// A term with no spoken forms.
    pub fn plain(term: impl Into<String>) -> Self {
        Self { term: term.into(), spoken: Vec::new(), lang: None }
    }
}

/// The terms alone, in the order of `entries`: what ASR takes as hotwords and polish as its vocabulary.
pub fn wordbook_terms(entries: &[WordbookEntry]) -> Vec<String> {
    entries.iter().map(|entry| entry.term.clone()).collect()
}

/// The spoken forms typed in one field: split at ASCII, Japanese and Chinese commas, trimmed, without repeats, in
/// order.
pub fn parse_spoken_list(input: &str) -> Vec<String> {
    deduped(input.split([',', '\u{3001}', '\u{FF0C}'])).into_iter().map(str::to_owned).collect()
}

const INSERT_WORD: &str = "INSERT OR IGNORE INTO wordbook (term, created) VALUES (?1, ?2)";
const INSERT_SPOKEN: &str =
    "INSERT OR IGNORE INTO wordbook_spoken (term, spoken, lang, created) VALUES (?1, ?2, ?3, ?4)";

impl Store {
    /// Returns false when the term already exists (or is blank).
    pub fn add_word(&self, term: &str, now: i64) -> Result<bool> {
        self.add_word_entry(term, &[], None, now)
    }

    /// Adds every term as it is written, or on an error none of them. Returns how many were new: a blank or known
    /// term adds nothing. Nothing in a term is read as the format of an imported line.
    pub fn add_words(&self, terms: &[String], now: i64) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        {
            let mut insert = tx.prepare(INSERT_WORD)?;
            for term in terms {
                added += usize::from(insert_word(&mut insert, term, now)?);
            }
        }
        tx.commit()?;
        Ok(added)
    }

    /// Adds the term with its spoken forms. Returns false, writing nothing, when the term already exists or is
    /// blank, so a known term keeps the forms it has.
    pub fn add_word_entry(&self, term: &str, spoken: &[String], lang: Option<Lang>, now: i64) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let added = add_entry(&tx, term, spoken, lang, now)?;
        tx.commit()?;
        Ok(added)
    }

    /// Rewrites the entry stored as `old_term`. Returns false, writing nothing, when that entry is gone, `new_term`
    /// is blank, or another entry already has `new_term`. A rename keeps the time the entry was added.
    pub fn update_word_entry(
        &self,
        old_term: &str,
        new_term: &str,
        spoken: &[String],
        lang: Option<Lang>,
        now: i64,
    ) -> Result<bool> {
        let new_term = new_term.trim();
        if new_term.is_empty() {
            return Ok(false);
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let stored = |term: &str| -> Result<bool> {
            let count: i64 =
                tx.query_row("SELECT COUNT(term) FROM wordbook WHERE term = ?1", [term], |row| row.get(0))?;
            Ok(count > 0)
        };
        let renamed = new_term != old_term;
        let updated = stored(old_term)? && !(renamed && stored(new_term)?);
        if updated {
            if renamed {
                tx.execute("UPDATE wordbook SET term = ?1 WHERE term = ?2", params![new_term, old_term])?;
            }
            tx.execute("DELETE FROM wordbook_spoken WHERE term = ?1", [old_term])?;
            insert_spoken(&tx, new_term, spoken, lang, now)?;
        }
        tx.commit()?;
        Ok(updated)
    }

    /// Removes each term with its spoken forms: all of them or none.
    pub fn remove_words(&self, terms: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut spoken = tx.prepare("DELETE FROM wordbook_spoken WHERE term=?1")?;
            let mut word = tx.prepare("DELETE FROM wordbook WHERE term=?1")?;
            for term in terms {
                spoken.execute([term])?;
                word.execute([term])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn words(&self, query: Option<&str>) -> Result<Vec<String>> {
        let c = self.conn();
        let mut stmt =
            c.prepare("SELECT term FROM wordbook WHERE term LIKE ?1 ESCAPE '\\' ORDER BY created DESC, term")?;
        Ok(stmt.query_map([contains(query)], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?)
    }

    /// The entries whose term, or one of its spoken forms, contains `query`, newest first.
    pub fn wordbook_entries(&self, query: Option<&str>) -> Result<Vec<WordbookEntry>> {
        let c = self.conn();
        let mut entries: Vec<WordbookEntry> = {
            let mut stmt = c.prepare(
                "SELECT term FROM wordbook
                 WHERE term LIKE ?1 ESCAPE '\\' OR EXISTS (
                   SELECT 1 FROM wordbook_spoken
                   WHERE wordbook_spoken.term = wordbook.term AND spoken LIKE ?1 ESCAPE '\\'
                 )
                 ORDER BY created DESC, term",
            )?;
            let terms = stmt.query_map([contains(query)], |row| row.get::<_, String>(0))?;
            terms.map(|term| term.map(WordbookEntry::plain)).collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut forms: HashMap<String, (Vec<String>, Option<Lang>)> = HashMap::new();
        {
            let mut stmt =
                c.prepare("SELECT term, spoken, lang FROM wordbook_spoken ORDER BY term, created, spoken")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let (term, spoken, lang): (String, String, Option<String>) = (row.get(0)?, row.get(1)?, row.get(2)?);
                let entry = forms.entry(term).or_insert_with(|| (Vec::new(), lang.as_deref().and_then(lang_of)));
                entry.0.push(spoken);
            }
        }
        for entry in &mut entries {
            if let Some((spoken, lang)) = forms.remove(&entry.term) {
                entry.spoken = spoken;
                entry.lang = lang;
            }
        }
        Ok(entries)
    }

    /// How many words [`words`](Self::words) returns without a query. `COUNT(term)` skips a NULL term the way its
    /// `LIKE` does.
    pub fn word_count(&self) -> Result<usize> {
        let count: i64 = self.conn().query_row("SELECT COUNT(term) FROM wordbook", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    /// Adds every new line or, on an error, none of them. Returns how many were added. A line is a term, or a term
    /// with its spoken forms after `=` (`1on1 = one on one`), scoped to a language with `[ja]`, `[en]` or `[zh]`
    /// after the term (`MTG [ja] = meeting, meet-up`). Inside a term a backslash escapes the `\`, `=` or `[` that
    /// follows it, which is how a term that holds one of them, or ends like a language tag, is written.
    pub fn import_words(&self, text: &str, now: i64) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        for line in text.lines() {
            let (term, spoken, lang) = parse_line(line);
            added += usize::from(add_entry(&tx, &term, &spoken, lang, now)?);
        }
        tx.commit()?;
        Ok(added)
    }

    /// One line per entry in the format [`import_words`](Self::import_words) reads, in term order. A term gets a
    /// backslash only before a `\` or `=` it holds, or before the bracket of a trailing `[ja]`, `[en]` or `[zh]`
    /// when it has spoken forms and no scope, which would else read as one.
    pub fn export_words(&self) -> Result<String> {
        let mut entries = self.wordbook_entries(None)?;
        entries.sort_unstable_by(|a, b| a.term.cmp(&b.term));
        Ok(entries.iter().map(export_line).collect::<Vec<_>>().join("\n"))
    }
}

/// Writes the term with its spoken forms, unless the term is blank or already stored. Returns whether it was written.
fn add_entry(conn: &Connection, term: &str, spoken: &[String], lang: Option<Lang>, now: i64) -> Result<bool> {
    let mut insert = conn.prepare(INSERT_WORD)?;
    if !insert_word(&mut insert, term, now)? {
        return Ok(false);
    }
    insert_spoken(conn, term.trim(), spoken, lang, now)?;
    Ok(true)
}

/// Writes the term as it is written, unless it is blank or already stored. Returns whether it was written.
fn insert_word(insert: &mut Statement<'_>, term: &str, now: i64) -> Result<bool> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(false);
    }
    Ok(insert.execute(params![term, now])? > 0)
}

/// Writes each spoken form once. `created` grows by one per form, so they keep their order when they are read.
fn insert_spoken(conn: &Connection, term: &str, spoken: &[String], lang: Option<Lang>, now: i64) -> Result<()> {
    let code = lang_code(lang);
    let mut insert = conn.prepare(INSERT_SPOKEN)?;
    for (index, form) in deduped(spoken.iter().map(String::as_str)).into_iter().enumerate() {
        insert.execute(params![term, form, code, now + index as i64])?;
    }
    Ok(())
}

/// `forms` trimmed and in order, without a blank one or a repeat.
fn deduped<'a>(forms: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut unique: Vec<&str> = Vec::new();
    for form in forms.into_iter().map(str::trim).filter(|form| !form.is_empty()) {
        if !unique.contains(&form) {
            unique.push(form);
        }
    }
    unique
}

/// One line of an imported file: the term, its spoken forms and their language. The forms after the `=` are read as
/// they are written.
fn parse_line(line: &str) -> (String, Vec<String>, Option<Lang>) {
    let Some(at) = unescaped_equals(line) else {
        return (unescape(line), Vec::new(), None);
    };
    let (term, lang) = split_tag(line[..at].trim_end());
    (unescape(term), parse_spoken_list(&line[at + 1..]), lang)
}

/// The term on the left of the `=` of a line, and the language its unescaped trailing tag names (`MTG [ja]`).
fn split_tag(left: &str) -> (&str, Option<Lang>) {
    let tag = (left.len() >= 4 && left.ends_with(']') && !is_escaped(left, left.len() - 1))
        .then(|| left.get(left.len() - 4..))
        .flatten()
        .filter(|tag| tag.starts_with('[') && !is_escaped(left, left.len() - 4))
        .and_then(|tag| lang_of(tag.get(1..3)?));
    match tag {
        Some(lang) => (&left[..left.len() - 4], Some(lang)),
        None => (left, None),
    }
}

fn export_line(entry: &WordbookEntry) -> String {
    let term = escape_term(&entry.term);
    if entry.spoken.is_empty() {
        return term;
    }
    let tag = entry.lang.map_or(String::new(), |lang| format!(" [{}]", lang.code()));
    // With no tag of its own after it, a term that ends like one would read as scoped.
    let term = if entry.lang.is_none() { escape_tag(term) } else { term };
    format!("{term}{tag} = {}", entry.spoken.join(", "))
}

/// The term as a line writes it, with each `\` and `=` it holds escaped.
fn escape_term(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len());
    for c in term.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '=' => escaped.push_str("\\="),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// The term with the `[` of its trailing `[ja]`, `[en]` or `[zh]` escaped.
fn escape_tag(mut term: String) -> String {
    let tagged = term.len().checked_sub(4).is_some_and(|at| {
        term.as_bytes()[at] == b'[' && term.ends_with(']') && term.get(at + 1..at + 3).and_then(lang_of).is_some()
    });
    if tagged {
        term.insert(term.len() - 4, '\\');
    }
    term
}

/// `text` without the backslashes that escape a `\`, `=` or `[` in it. Before any other character a backslash is
/// part of the text, so a list written before terms were escaped reads as it was written.
fn unescape(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(escaped) = chars.next_if(|next| matches!(next, '\\' | '=' | '['))
        {
            plain.push(escaped);
        } else {
            plain.push(c);
        }
    }
    plain
}

/// Where the first `=` of `text` stands that no backslash escapes.
fn unescaped_equals(text: &str) -> Option<usize> {
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next_if(|(_, next)| matches!(next, '\\' | '=' | '['));
            }
            '=' => return Some(at),
            _ => {}
        }
    }
    None
}

/// Whether the character at `at` is escaped: an odd number of backslashes stands before it.
fn is_escaped(text: &str, at: usize) -> bool {
    text[..at].bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 == 1
}

/// The stored language of a spoken form, and the language a tag names: only a language Dito has polish rules for
/// can be scoped to.
fn lang_code(lang: Option<Lang>) -> Option<&'static str> {
    lang.filter(|lang| matches!(lang, Lang::Ja | Lang::En | Lang::Zh)).map(Lang::code)
}

fn lang_of(code: &str) -> Option<Lang> {
    match code.to_ascii_lowercase().as_str() {
        "ja" => Some(Lang::Ja),
        "en" => Some(Lang::En),
        "zh" => Some(Lang::Zh),
        _ => None,
    }
}

/// The pattern for text that contains the search: `%`, `_` and `\` in the search match themselves.
fn contains(query: Option<&str>) -> String {
    let query = query.unwrap_or("").trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{query}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(spoken: &[&str]) -> Vec<String> {
        spoken.iter().map(|form| form.to_string()).collect()
    }

    fn entry(term: &str, spoken: &[&str], lang: Option<Lang>) -> WordbookEntry {
        WordbookEntry { term: term.to_owned(), spoken: forms(spoken), lang }
    }

    fn spoken_rows(s: &Store) -> i64 {
        s.conn().query_row("SELECT COUNT(*) FROM wordbook_spoken", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn wordbook_dedupes_trims_and_round_trips_import_export() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.add_word("  Acme ", 1).unwrap());
        assert!(!s.add_word("Acme", 2).unwrap());
        assert!(!s.add_word("   ", 3).unwrap());
        assert_eq!(s.import_words("Contoso\nAcme\n\nBedrock\n", 4).unwrap(), 2);
        assert_eq!(s.export_words().unwrap(), "Acme\nBedrock\nContoso");
        s.remove_words(&["Contoso".into(), "Acme".into(), "missing".into()]).unwrap();
        assert_eq!(s.words(None).unwrap(), ["Bedrock"]);
    }

    #[test]
    fn wordbook_import_counts_only_new_words_and_leaves_known_ones_as_they_were() {
        let s = Store::open_in_memory().unwrap();
        s.add_word("Acme", 1).unwrap();
        assert_eq!(s.import_words("Contoso\n\n  Bedrock \nAcme\nContoso\n   \nDito", 2).unwrap(), 3);
        // Newest first, so a known word that kept its time comes last.
        assert_eq!(s.words(None).unwrap(), ["Bedrock", "Contoso", "Dito", "Acme"]);
    }

    #[test]
    fn wordbook_batch_deletion_rolls_back_on_error() {
        let s = Store::open_in_memory().unwrap();
        s.import_words("Acme = ack-me\nContoso", 1).unwrap();
        s.conn()
            .execute_batch(
                "CREATE TRIGGER reject_delete BEFORE DELETE ON wordbook
                 WHEN OLD.term = 'Contoso' BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .unwrap();

        assert!(s.remove_words(&["Acme".into(), "Contoso".into()]).is_err());
        assert_eq!(s.wordbook_entries(None).unwrap(), [entry("Acme", &["ack-me"], None), entry("Contoso", &[], None)]);
    }

    #[test]
    fn word_count_matches_the_unfiltered_word_list() {
        let s = Store::open_in_memory().unwrap();
        let listed = |s: &Store| s.words(None).unwrap().len();
        assert_eq!((s.word_count().unwrap(), listed(&s)), (0, 0));
        s.import_words("Acme\nContoso\nBedrock", 1).unwrap();
        s.add_word("Acme", 2).unwrap();
        assert_eq!((s.word_count().unwrap(), listed(&s)), (3, 3));
        s.remove_words(&["Contoso".into()]).unwrap();
        assert_eq!((s.word_count().unwrap(), listed(&s)), (2, 2));
        s.conn().execute("INSERT INTO wordbook (term, created) VALUES (NULL, 3)", []).unwrap();
        assert_eq!((s.word_count().unwrap(), listed(&s)), (2, 2));
        assert_eq!(s.wordbook_entries(None).unwrap().len(), 2, "a NULL term is left out like it is by its LIKE");
    }

    #[test]
    fn spoken_forms_are_split_at_every_comma_trimmed_and_kept_once() {
        assert_eq!(
            parse_spoken_list(" meeting ,ミーティング、meeting， ,one on one"),
            forms(&["meeting", "ミーティング", "one on one"])
        );
        assert_eq!(parse_spoken_list("  \n"), Vec::<String>::new());
    }

    #[test]
    fn an_entry_keeps_its_spoken_forms_their_language_and_their_order() {
        let s = Store::open_in_memory().unwrap();
        let added = s.add_word_entry(" MTG ", &forms(&[" meeting ", "ミーティング", "meeting", ""]), Some(Lang::Ja), 1);
        assert!(added.unwrap());
        assert!(!s.add_word_entry("MTG", &forms(&["stand-up"]), None, 2).unwrap(), "a known term keeps its forms");
        assert!(!s.add_word_entry("   ", &forms(&["x"]), None, 3).unwrap());
        assert_eq!(s.wordbook_entries(None).unwrap(), [entry("MTG", &["meeting", "ミーティング"], Some(Lang::Ja))]);
        assert_eq!(s.words(None).unwrap(), ["MTG"]);
    }

    #[test]
    fn an_entry_is_rewritten_with_other_forms_another_language_or_another_term() {
        let s = Store::open_in_memory().unwrap();
        s.import_words("Acme\nMTG [ja] = meeting, ミーティング", 1).unwrap();
        s.add_word("Mid", 3).unwrap();
        let stored = |s: &Store| s.wordbook_entries(None).unwrap();

        assert!(s.update_word_entry("MTG", "MTG", &forms(&["meetings"]), Some(Lang::En), 5).unwrap());
        assert_eq!(stored(&s)[2], entry("MTG", &["meetings"], Some(Lang::En)));

        assert!(s.update_word_entry("MTG", "  MTGs ", &forms(&["meeting", "ミーティング"]), None, 6).unwrap());
        // Newest first: the renamed entry keeps its time, so it stays behind the word added after it.
        assert_eq!(
            stored(&s),
            [entry("Mid", &[], None), entry("Acme", &[], None), entry("MTGs", &["meeting", "ミーティング"], None)]
        );
        assert_eq!(spoken_rows(&s), 2, "the forms of the old term are gone");

        assert!(!s.update_word_entry("MTGs", "Acme", &[], None, 7).unwrap(), "another entry already has that term");
        assert_eq!((stored(&s).len(), spoken_rows(&s)), (3, 2));
        assert_eq!(stored(&s)[2], entry("MTGs", &["meeting", "ミーティング"], None));
        assert!(!s.update_word_entry("missing", "New", &[], None, 8).unwrap());
        assert!(!s.update_word_entry("Acme", "  ", &forms(&["x"]), None, 8).unwrap());

        assert!(s.update_word_entry("MTGs", "MTGs", &[], None, 9).unwrap());
        assert_eq!(stored(&s)[2], WordbookEntry::plain("MTGs"));
        assert_eq!(spoken_rows(&s), 0);
    }

    #[test]
    fn removing_a_word_removes_its_spoken_forms() {
        let s = Store::open_in_memory().unwrap();
        s.import_words("MTG [ja] = meeting, ミーティング\n1on1 = ワンオンワン", 1).unwrap();
        s.remove_words(&["MTG".to_string(), "missing".to_string()]).unwrap();
        assert_eq!(s.wordbook_entries(None).unwrap(), [entry("1on1", &["ワンオンワン"], None)]);
        assert_eq!(spoken_rows(&s), 1);
    }

    #[test]
    fn search_matches_the_term_or_a_spoken_form() {
        let s = Store::open_in_memory().unwrap();
        s.import_words("Acme\nMTG [ja] = meeting, ミーティング\n1on1 = ワンオンワン", 1).unwrap();
        let find = |query: &str| {
            s.wordbook_entries(Some(query)).unwrap().iter().map(|entry| entry.term.clone()).collect::<Vec<_>>()
        };
        assert_eq!(find("meeting"), ["MTG"]);
        assert_eq!(find("ミーティング"), ["MTG"]);
        assert_eq!(find("ワン"), ["1on1"]);
        assert_eq!(find("1on"), ["1on1"]);
        assert_eq!(find("ac"), ["Acme"]);
        assert_eq!(find("zzz"), Vec::<String>::new());
        assert_eq!(find("  ").len(), 3);
    }

    #[test]
    fn import_and_export_round_trip_plain_spoken_and_scoped_lines() {
        let s = Store::open_in_memory().unwrap();
        let text = "Acme\n1on1 = ワンオンワン\nMTG [ja] = meeting, ミーティング\nPR [EN] = pull request\nMTG [fr] = stand-up\n";
        assert_eq!(s.import_words(text, 1).unwrap(), 5);
        let stored = [
            entry("1on1", &["ワンオンワン"], None),
            entry("Acme", &[], None),
            entry("MTG", &["meeting", "ミーティング"], Some(Lang::Ja)),
            entry("MTG [fr]", &["stand-up"], None),
            entry("PR", &["pull request"], Some(Lang::En)),
        ];
        assert_eq!(s.wordbook_entries(None).unwrap(), stored);

        let exported = s.export_words().unwrap();
        assert_eq!(
            exported,
            "1on1 = ワンオンワン\nAcme\nMTG [ja] = meeting, ミーティング\nMTG [fr] = stand-up\nPR [en] = pull request"
        );
        let again = Store::open_in_memory().unwrap();
        assert_eq!(again.import_words(&exported, 2).unwrap(), 5);
        assert_eq!(again.wordbook_entries(None).unwrap(), stored);
        assert_eq!(again.export_words().unwrap(), exported);
    }

    #[test]
    fn a_batch_of_terms_is_added_as_written() {
        let s = Store::open_in_memory().unwrap();
        let terms = forms(&["A=B", " x [ja] ", "C:\\path", "E = mc^2 [en]", "A=B", " "]);
        assert_eq!(s.add_words(&terms, 1).unwrap(), 4);
        let stored = [
            entry("A=B", &[], None),
            entry("C:\\path", &[], None),
            entry("E = mc^2 [en]", &[], None),
            entry("x [ja]", &[], None),
        ];
        assert_eq!(s.wordbook_entries(None).unwrap(), stored);
        assert_eq!(s.export_words().unwrap(), "A\\=B\nC:\\\\path\nE \\= mc^2 [en]\nx [ja]");
    }

    #[test]
    fn export_and_import_keep_an_equals_sign_a_backslash_and_a_trailing_tag_in_a_term() {
        let s = Store::open_in_memory().unwrap();
        let text = "A\\=B\nfoo [ja]\nbar \\[ja] = baz, qux\nC:\\\\Users\\\\name\n1on1 = ワンオンワン\nMTG [ja] = meeting, ミーティング\n";
        assert_eq!(s.import_words(text, 1).unwrap(), 6);
        let stored = [
            entry("1on1", &["ワンオンワン"], None),
            entry("A=B", &[], None),
            entry("C:\\Users\\name", &[], None),
            entry("MTG", &["meeting", "ミーティング"], Some(Lang::Ja)),
            entry("bar [ja]", &["baz", "qux"], None),
            entry("foo [ja]", &[], None),
        ];
        assert_eq!(s.wordbook_entries(None).unwrap(), stored);

        // Only a term that needs it gets a backslash; the other lines read as before.
        let exported = s.export_words().unwrap();
        assert_eq!(
            exported,
            "1on1 = ワンオンワン\nA\\=B\nC:\\\\Users\\\\name\nMTG [ja] = meeting, ミーティング\nbar \\[ja] = baz, qux\nfoo [ja]"
        );
        let again = Store::open_in_memory().unwrap();
        assert_eq!(again.import_words(&exported, 2).unwrap(), 6);
        assert_eq!(again.wordbook_entries(None).unwrap(), stored);
        assert_eq!(again.export_words().unwrap(), exported);

        // A backslash of its own before the bracket, and a scope after a term that ends like a tag.
        let tricky = Store::open_in_memory().unwrap();
        let text = "a\\\\\\[ja] = x\nb [ja] [en] = y\n";
        assert_eq!(tricky.import_words(text, 1).unwrap(), 2);
        assert_eq!(
            tricky.wordbook_entries(None).unwrap(),
            [entry("a\\[ja]", &["x"], None), entry("b [ja]", &["y"], Some(Lang::En))]
        );
        assert_eq!(tricky.export_words().unwrap(), text.trim_end());
    }

    #[test]
    fn a_search_matches_the_wildcards_as_text() {
        let s = Store::open_in_memory().unwrap();
        s.add_words(&forms(&["a_b", "axb", "100%", "back\\slash"]), 1).unwrap();
        s.add_word_entry("T", &forms(&["x_y"]), None, 1).unwrap();
        let words = |query: &str| s.words(Some(query)).unwrap();
        assert_eq!(words("x_"), Vec::<String>::new());
        assert_eq!(words("_"), ["a_b"]);
        assert_eq!(words("%"), ["100%"]);
        assert_eq!(words("\\"), ["back\\slash"]);
        let found = |query: &str| {
            s.wordbook_entries(Some(query)).unwrap().iter().map(|entry| entry.term.clone()).collect::<Vec<_>>()
        };
        assert_eq!(found("_"), ["T", "a_b"]);
        assert_eq!(found("xy"), Vec::<String>::new());
    }
}

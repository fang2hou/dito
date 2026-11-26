//! SQLite-backed wordbook.

use std::collections::HashMap;

use rusqlite::{Connection, params};

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

/// The spoken forms typed in one field: split at ASCII and Japanese commas, trimmed, without repeats, in order.
pub fn parse_spoken_list(input: &str) -> Vec<String> {
    deduped(input.split([',', '\u{3001}'])).into_iter().map(str::to_owned).collect()
}

const INSERT_WORD: &str = "INSERT OR IGNORE INTO wordbook (term, created) VALUES (?1, ?2)";
const INSERT_SPOKEN: &str =
    "INSERT OR IGNORE INTO wordbook_spoken (term, spoken, lang, created) VALUES (?1, ?2, ?3, ?4)";

impl Store {
    /// Returns false when the term already exists (or is blank).
    pub fn add_word(&self, term: &str, now: i64) -> Result<bool> {
        self.add_word_entry(term, &[], None, now)
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
        let like = format!("%{}%", query.unwrap_or("").trim());
        let mut stmt = c.prepare("SELECT term FROM wordbook WHERE term LIKE ?1 ORDER BY created DESC, term")?;
        Ok(stmt.query_map([like], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?)
    }

    /// The entries whose term, or one of its spoken forms, contains `query`, newest first.
    pub fn wordbook_entries(&self, query: Option<&str>) -> Result<Vec<WordbookEntry>> {
        let c = self.conn();
        let like = format!("%{}%", query.unwrap_or("").trim());
        let mut entries: Vec<WordbookEntry> = {
            let mut stmt = c.prepare(
                "SELECT term FROM wordbook
                 WHERE term LIKE ?1 OR EXISTS (
                   SELECT 1 FROM wordbook_spoken
                   WHERE wordbook_spoken.term = wordbook.term AND spoken LIKE ?1
                 )
                 ORDER BY created DESC, term",
            )?;
            let terms = stmt.query_map([like], |row| row.get::<_, String>(0))?;
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
    /// after the term (`MTG [ja] = meeting, meet-up`).
    pub fn import_words(&self, text: &str, now: i64) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        for line in text.lines() {
            let (term, spoken, lang) = parse_line(line);
            added += usize::from(add_entry(&tx, term, &spoken, lang, now)?);
        }
        tx.commit()?;
        Ok(added)
    }

    /// One line per entry in the format [`import_words`](Self::import_words) reads, in term order.
    pub fn export_words(&self) -> Result<String> {
        let mut entries = self.wordbook_entries(None)?;
        entries.sort_unstable_by(|a, b| a.term.cmp(&b.term));
        Ok(entries.iter().map(export_line).collect::<Vec<_>>().join("\n"))
    }
}

/// Writes the term with its spoken forms, unless the term is blank or already stored. Returns whether it was written.
fn add_entry(conn: &Connection, term: &str, spoken: &[String], lang: Option<Lang>, now: i64) -> Result<bool> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(false);
    }
    let mut insert = conn.prepare(INSERT_WORD)?;
    if insert.execute(params![term, now])? == 0 {
        return Ok(false);
    }
    insert_spoken(conn, term, spoken, lang, now)?;
    Ok(true)
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

/// One line of an imported file: the term, its spoken forms and their language.
fn parse_line(line: &str) -> (&str, Vec<String>, Option<Lang>) {
    let Some((left, right)) = line.split_once('=') else {
        return (line, Vec::new(), None);
    };
    let left = left.trim();
    let tagged = left.strip_suffix(']').and_then(|rest| rest.rsplit_once('['));
    let (term, lang) = match tagged {
        Some((term, tag)) if tag.trim().eq_ignore_ascii_case("ja") => (term, Some(Lang::Ja)),
        Some((term, tag)) if tag.trim().eq_ignore_ascii_case("en") => (term, Some(Lang::En)),
        Some((term, tag)) if tag.trim().eq_ignore_ascii_case("zh") => (term, Some(Lang::Zh)),
        _ => (left, None),
    };
    (term.trim(), parse_spoken_list(right), lang)
}

fn export_line(entry: &WordbookEntry) -> String {
    match entry.spoken.as_slice() {
        [] => entry.term.clone(),
        spoken => {
            let tag = entry.lang.map_or(String::new(), |lang| format!(" [{}]", lang.code()));
            format!("{}{tag} = {}", entry.term, spoken.join(", "))
        }
    }
}

/// The stored language of a spoken form: only a language Dito has polish rules for can be scoped to.
fn lang_code(lang: Option<Lang>) -> Option<&'static str> {
    lang.filter(|lang| matches!(lang, Lang::Ja | Lang::En | Lang::Zh)).map(Lang::code)
}

fn lang_of(code: &str) -> Option<Lang> {
    match code {
        "ja" => Some(Lang::Ja),
        "en" => Some(Lang::En),
        "zh" => Some(Lang::Zh),
        _ => None,
    }
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
    fn spoken_forms_are_split_at_both_commas_trimmed_and_kept_once() {
        assert_eq!(
            parse_spoken_list(" meeting ,ミーティング、meeting , ,one on one"),
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
}

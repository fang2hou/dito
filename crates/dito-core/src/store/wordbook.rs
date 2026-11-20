//! SQLite-backed wordbook.

use rusqlite::{Statement, params};

use super::*;

impl Store {
    /// Returns false when the term already exists (or is blank).
    pub fn add_word(&self, term: &str, now: i64) -> Result<bool> {
        let conn = self.conn();
        insert_word(&mut conn.prepare(INSERT_WORD)?, term, now)
    }

    pub fn remove_words(&self, terms: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut delete = tx.prepare("DELETE FROM wordbook WHERE term=?1")?;
            for term in terms {
                delete.execute([term])?;
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

    /// How many words [`words`](Self::words) returns without a query. `COUNT(term)` skips a NULL term the way its
    /// `LIKE` does.
    pub fn word_count(&self) -> Result<usize> {
        let count: i64 = self.conn().query_row("SELECT COUNT(term) FROM wordbook", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    /// Adds every new line or, on an error, none of them. Returns how many were added.
    pub fn import_words(&self, text: &str, now: i64) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut added = 0;
        {
            let mut insert = tx.prepare(INSERT_WORD)?;
            for line in text.lines() {
                if insert_word(&mut insert, line, now)? {
                    added += 1;
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn export_words(&self) -> Result<String> {
        let mut words = self.words(None)?;
        words.sort();
        Ok(words.join("\n"))
    }
}

const INSERT_WORD: &str = "INSERT OR IGNORE INTO wordbook (term, created) VALUES (?1, ?2)";

fn insert_word(insert: &mut Statement<'_>, term: &str, now: i64) -> Result<bool> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(false);
    }
    Ok(insert.execute(params![term, now])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        s.import_words("Acme\nContoso", 1).unwrap();
        s.conn()
            .execute_batch(
                "CREATE TRIGGER reject_delete BEFORE DELETE ON wordbook
                 WHEN OLD.term = 'Contoso' BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .unwrap();

        assert!(s.remove_words(&["Acme".into(), "Contoso".into()]).is_err());
        assert_eq!(s.words(None).unwrap(), ["Acme", "Contoso"]);
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
    }
}

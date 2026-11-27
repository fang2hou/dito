//! The wordbook a session uses. It is read again once the store's wordbook changes, so a word added while recording
//! applies from the next utterance and the next paragraph polished.

use std::sync::{Arc, Mutex};

use crate::spoken_forms::SpokenForms;
use crate::store::{Store, wordbook_terms};

pub(super) struct SessionWordbook {
    store: Arc<Store>,
    for_polish: bool,
    current: Mutex<Arc<WordbookSnapshot>>,
}

pub(super) struct WordbookSnapshot {
    revision: u64,
    /// The terms, passed to ASR as hotwords.
    pub hotwords: Vec<String>,
    /// The terms the polish prompt names, empty when the wordbook is not used for polish.
    pub polish_terms: Vec<String>,
    /// Compiled once per revision: the recognizer asks for it on every partial, about each second.
    pub forms: SpokenForms,
}

impl SessionWordbook {
    pub fn load(store: Arc<Store>, for_polish: bool) -> Self {
        let snapshot = read(&store, for_polish).unwrap_or_else(|| empty(store.wordbook_revision()));
        Self { store, for_polish, current: Mutex::new(Arc::new(snapshot)) }
    }

    pub fn current(&self) -> Arc<WordbookSnapshot> {
        let mut current = self.current.lock().expect("session wordbook mutex poisoned");
        if current.revision != self.store.wordbook_revision()
            && let Some(snapshot) = read(&self.store, self.for_polish)
        {
            *current = Arc::new(snapshot);
        }
        current.clone()
    }
}

/// `None` when the wordbook cannot be read; the session then keeps the words it has and tries again on the next call.
fn read(store: &Store, for_polish: bool) -> Option<WordbookSnapshot> {
    // Taken before the read: a write in between leaves the snapshot one revision behind, so it is read again.
    let revision = store.wordbook_revision();
    let entries = match store.wordbook_entries(None) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(operation = "load wordbook", %error, "keeping the words the session has");
            return None;
        }
    };
    let hotwords = wordbook_terms(&entries);
    let polish_terms = if for_polish { hotwords.clone() } else { Vec::new() };
    Some(WordbookSnapshot { revision, hotwords, polish_terms, forms: SpokenForms::from_entries(&entries) })
}

fn empty(revision: u64) -> WordbookSnapshot {
    WordbookSnapshot { revision, hotwords: Vec::new(), polish_terms: Vec::new(), forms: SpokenForms::from_entries(&[]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_added_while_recording_is_used_by_the_next_utterance() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store.add_word("Acme", 1).unwrap();
        let wordbook = SessionWordbook::load(store.clone(), false);
        assert_eq!(wordbook.current().hotwords, ["Acme"]);
        assert!(wordbook.current().polish_terms.is_empty(), "the wordbook is off for polish");

        store.add_word_entry("MTG", &["meeting".to_string()], None, 2).unwrap();
        let snapshot = wordbook.current();
        assert_eq!(snapshot.hotwords.len(), 2);
        assert_eq!(snapshot.forms.apply_for_lang("the meeting", None), "the MTG");

        store.remove_words(&["Acme".to_string()]).unwrap();
        assert_eq!(wordbook.current().hotwords, ["MTG"]);
    }
}

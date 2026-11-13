//! Meaningful, editor-scoped history, independent of draft persistence.

use dito_core::prompts::AppRules;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Editor {
    Common,
    Application,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Snapshot {
    Common(String),
    Application(AppRules),
}

impl Snapshot {
    fn text(&self) -> String {
        match self {
            Self::Common(text) => text.clone(),
            Self::Application(prompt) => format!("{}\n{}", prompt.instruction, prompt.samples.join("\n")),
        }
    }
}

pub(super) struct History {
    entries: Vec<Snapshot>,
    cursor: usize,
    current: Snapshot,
}

impl History {
    pub fn new(initial: Snapshot) -> Self {
        Self { entries: vec![initial.clone()], cursor: 0, current: initial }
    }

    pub fn change(&mut self, value: Snapshot) {
        if self.current == value {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.current = value;
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0 || self.current != self.entries[self.cursor]
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// Small corrections stay in the current group. Explicit undo and replacements keep even tiny edits.
    pub fn checkpoint(&mut self, force: bool) {
        if self.current == self.entries[self.cursor] {
            return;
        }
        if !force && changed_characters(&self.entries[self.cursor].text(), &self.current.text()) < 8 {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(self.current.clone());
        if self.entries.len() > 100 {
            self.entries.remove(1);
        }
        self.cursor = self.entries.len() - 1;
    }

    pub fn undo(&mut self) -> Option<Snapshot> {
        self.checkpoint(true);
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.current = self.entries[self.cursor].clone();
        Some(self.current.clone())
    }

    pub fn redo(&mut self) -> Option<Snapshot> {
        if !self.can_redo() {
            return None;
        }
        self.cursor += 1;
        self.current = self.entries[self.cursor].clone();
        Some(self.current.clone())
    }
}

fn changed_characters(before: &str, after: &str) -> usize {
    let before: Vec<_> = before.chars().collect();
    let after: Vec<_> = after.chars().collect();
    let prefix = before.iter().zip(&after).take_while(|(a, b)| a == b).count();
    let suffix = before[prefix..].iter().rev().zip(after[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    (before.len() - prefix - suffix).max(after.len() - prefix - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(text: &str) -> Snapshot {
        Snapshot::Common(text.into())
    }

    #[test]
    fn tiny_edits_coalesce_but_undo_keeps_the_original_and_redo_is_invalidated() {
        let mut h = History::new(value("original"));
        h.change(value("original!"));
        h.checkpoint(false);
        h.change(value("original!!"));
        h.checkpoint(false);
        assert_eq!(h.undo(), Some(value("original")));
        assert_eq!(h.redo(), Some(value("original!!")));
        h.undo();
        h.change(value("different"));
        assert!(!h.can_redo());
        assert_eq!(h.undo(), Some(value("original")));
    }

    #[test]
    fn app_rules_and_examples_restore_together_and_reset_is_one_undoable_change() {
        let initial = Snapshot::Application(AppRules { instruction: "rule".into(), samples: vec!["example".into()] });
        let mut h = History::new(initial.clone());
        h.change(Snapshot::Application(AppRules {
            instruction: "a longer new instruction".into(),
            samples: vec!["new example".into()],
        }));
        h.checkpoint(false);
        let edited = h.current.clone();
        h.change(initial);
        h.checkpoint(true);
        assert_eq!(h.undo(), Some(edited));
    }
}

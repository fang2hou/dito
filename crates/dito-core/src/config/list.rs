//! The editing behavior the polish configurations and the cloud speech profiles share: an ordered list of entries with
//! stable ids, and the id of the one in use.

use std::hash::{BuildHasher, RandomState};

/// One entry of such a list.
pub(super) trait Entry {
    fn id(&self) -> &str;
    fn name_mut(&mut self) -> &mut String;
}

/// A list and its selection, borrowed together so that one implementation edits both kinds of list.
pub(super) struct List<'a, T> {
    selected: &'a mut String,
    items: &'a mut Vec<T>,
}

impl<'a, T: Entry> List<'a, T> {
    pub(super) fn new(selected: &'a mut String, items: &'a mut Vec<T>) -> Self {
        Self { selected, items }
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
    }

    /// Make `id` the entry in use. An id no entry has changes nothing.
    pub(super) fn select(&mut self, id: &str) -> bool {
        let known = self.position(id).is_some();
        if known {
            *self.selected = id.to_owned();
        }
        known
    }

    /// An id that no entry of the list has: `base` and a random suffix. A new entry gets a fresh id every time, even
    /// when the plain `base` is free, because the Keychain account behind an id may still hold the key of an entry
    /// that was removed, and a new entry must never pick that key up.
    pub(super) fn fresh_id(&self, base: &str) -> String {
        loop {
            let id = format!("{base}-{:08x}", RandomState::new().hash_one(0_u8) as u32);
            if self.position(&id).is_none() {
                return id;
            }
        }
    }

    /// Append `item` and make it the entry in use.
    pub(super) fn push(&mut self, item: T) {
        *self.selected = item.id().to_owned();
        self.items.push(item);
    }

    /// Take `id` out of the list and return it. The last entry stays: a list is never empty. When the entry in use goes,
    /// its neighbour takes over.
    pub(super) fn remove(&mut self, id: &str) -> Option<T> {
        if self.items.len() < 2 {
            return None;
        }
        let at = self.position(id)?;
        let removed = self.items.remove(at);
        if self.position(self.selected).is_none() {
            // The entry that slid into the freed slot, or the new last one when the removed entry was last.
            *self.selected = self.items[at.min(self.items.len() - 1)].id().to_owned();
        }
        Some(removed)
    }

    /// Put `id` at `index` of the list as it is afterwards; an index past the end means the end.
    pub(super) fn move_to(&mut self, id: &str, index: usize) -> bool {
        let Some(from) = self.position(id) else { return false };
        let item = self.items.remove(from);
        self.items.insert(index.min(self.items.len()), item);
        true
    }

    /// Rename `id`. An entry always has a name, so a blank one is refused.
    pub(super) fn rename(&mut self, id: &str, name: &str) -> bool {
        if name.trim().is_empty() {
            return false;
        }
        let Some(at) = self.position(id) else { return false };
        *self.items[at].name_mut() = name.to_owned();
        true
    }
}

#[cfg(test)]
pub(super) mod contract {
    //! One behavior check, run against every list that offers the editing API.

    use std::collections::BTreeSet;

    pub(in crate::config) trait Editable {
        fn ids(&self) -> Vec<String>;
        fn selected(&self) -> String;
        fn name_of(&self, id: &str) -> Option<String>;
        fn select(&mut self, id: &str) -> bool;
        fn add(&mut self, name: &str) -> String;
        fn duplicate(&mut self, id: &str, name: &str) -> Option<String>;
        fn remove(&mut self, id: &str) -> bool;
        fn move_to(&mut self, id: &str, index: usize) -> bool;
        fn rename(&mut self, id: &str, name: &str) -> bool;
    }

    /// `list` starts with at least three entries.
    pub(in crate::config) fn check_editing<L: Editable>(list: &mut L) {
        let start = list.ids();
        assert!(start.len() >= 3);
        assert_eq!(start.iter().collect::<BTreeSet<_>>().len(), start.len(), "ids start out unique");

        // Selecting.
        assert!(list.select(&start[1]));
        assert_eq!(list.selected(), start[1]);
        assert!(!list.select("missing"));
        assert_eq!(list.selected(), start[1], "an unknown id changes nothing");

        // Adding and duplicating append, select the new entry and never reuse an id.
        let added = list.add("Added");
        let copy = list.duplicate(&start[0], "Copy").expect("the source exists");
        let second = list.add("Added too");
        assert_eq!(list.duplicate("missing", "Copy"), None);
        let ids = list.ids();
        assert_eq!(ids[start.len()..], [added.clone(), copy.clone(), second.clone()], "new entries join the end");
        assert_eq!(list.selected(), second);
        assert_eq!(list.name_of(&copy).as_deref(), Some("Copy"));
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len(), "every id is unique");
        assert!(ids.iter().all(|id| !id.is_empty()));

        // Renaming.
        assert!(list.rename(&added, "Renamed"));
        assert_eq!(list.name_of(&added).as_deref(), Some("Renamed"));
        assert!(!list.rename(&added, "  "));
        assert!(!list.rename(&added, ""));
        assert!(!list.rename("missing", "Name"));
        assert_eq!(list.name_of(&added).as_deref(), Some("Renamed"), "a blank name is refused");

        // Moving: the entry ends up at the index, the others keep their order, the selection stays.
        let before = list.ids();
        let moved = before[0].clone();
        assert!(list.move_to(&moved, 2));
        let after = list.ids();
        assert_eq!(after[2], moved);
        assert_eq!(after.len(), before.len());
        assert_eq!(after.iter().filter(|id| **id != moved).collect::<Vec<_>>(), before[1..].iter().collect::<Vec<_>>());
        assert!(list.move_to(&moved, 999));
        assert_eq!(list.ids().last(), Some(&moved), "an index past the end means the end");
        assert!(!list.move_to("missing", 0));
        assert_eq!(list.selected(), second);

        // Removing: the neighbour of the selected entry takes over, any other removal keeps the selection.
        let ids = list.ids();
        assert!(list.select(&ids[1]));
        assert!(list.remove(&ids[1]));
        assert_eq!(list.selected(), ids[2], "the entry that followed takes over");
        let last = list.ids().last().cloned().unwrap();
        assert!(list.select(&last));
        assert!(list.remove(&last));
        assert_eq!(list.selected(), *list.ids().last().unwrap(), "the new last entry takes over");
        let (keep, drop) = (list.selected(), list.ids()[0].clone());
        assert!(list.remove(&drop));
        assert_eq!(list.selected(), keep, "removing another entry keeps the selection");
        assert!(!list.remove("missing"));
        assert!(!list.ids().contains(&drop));

        // A removed id does not come back for the next new entry.
        let removed = [ids[1].clone(), last, drop];
        for _ in 0..20 {
            let id = list.add("New");
            assert!(!removed.contains(&id), "{id} was removed earlier");
        }

        // The last entry stays.
        while list.ids().len() > 1 {
            let id = list.ids()[0].clone();
            assert!(list.remove(&id));
        }
        let only = list.ids()[0].clone();
        assert!(!list.remove(&only));
        assert_eq!(list.ids().len(), 1);
        assert_eq!(list.selected(), only);
    }
}

//! The saved configurations of one kind and the one in use, shared by cloud speech and the cloud text model.

use std::hash::{BuildHasher, RandomState};

use serde::{Deserialize, Serialize};

/// What the list needs from a configuration.
pub trait Entry: Clone {
    /// What pre-fills a new entry.
    type Template: Copy;

    fn id(&self) -> &str;
    fn id_mut(&mut self) -> &mut String;
    fn name_mut(&mut self) -> &mut String;

    /// The first part of the ids made from `template`.
    fn template_id(template: Self::Template) -> &'static str;
    fn from_template(template: Self::Template, id: &str, name: &str) -> Self;

    /// The first part of the id of a copy of this entry.
    fn copy_id_base(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configurations<T> {
    pub selected: String,
    pub items: Vec<T>,
}

impl<T: Entry> Configurations<T> {
    fn position(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
    }

    pub fn get(&self, id: &str) -> Option<&T> {
        self.items.iter().find(|item| item.id() == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.items.iter_mut().find(|item| item.id() == id)
    }

    /// `None` when no configuration has the selected id: nothing takes its place, so the id, the content and the
    /// Keychain account always come from the same configuration.
    pub fn active(&self) -> Option<&T> {
        self.get(&self.selected)
    }

    pub fn select(&mut self, id: &str) -> bool {
        let known = self.position(id).is_some();
        if known {
            self.selected = id.to_owned();
        }
        known
    }

    /// An id that no entry of the list has: `base` and a random suffix. A new entry gets a fresh id every time, even
    /// when the plain `base` is free, because the Keychain account behind an id may still hold the key of an entry
    /// that was removed, and a new entry must never pick that key up.
    fn fresh_id(&self, base: &str) -> String {
        loop {
            let id = format!("{base}-{:08x}", RandomState::new().hash_one(0_u8) as u32);
            if self.position(&id).is_none() {
                return id;
            }
        }
    }

    fn push(&mut self, item: T) {
        self.selected = item.id().to_owned();
        self.items.push(item);
    }

    pub fn add_from_template(&mut self, template: T::Template, name: &str) -> String {
        let id = self.fresh_id(T::template_id(template));
        self.push(T::from_template(template, &id, name));
        id
    }

    /// The key is not copied: it belongs to the source's Keychain account.
    pub fn duplicate(&mut self, id: &str, name: &str) -> Option<String> {
        let mut copy = self.get(id)?.clone();
        let id = self.fresh_id(copy.copy_id_base());
        copy.id_mut().clone_from(&id);
        *copy.name_mut() = name.to_owned();
        self.push(copy);
        Some(id)
    }

    /// Take `id` out of the list and return it. The last entry stays: a list is never empty (`None`). When the entry
    /// in use goes, its neighbour takes over. The key stays in the Keychain: `ConfigStore` deletes it only once the
    /// list is saved.
    pub fn remove(&mut self, id: &str) -> Option<T> {
        if self.items.len() < 2 {
            return None;
        }
        let at = self.position(id)?;
        let removed = self.items.remove(at);
        if self.position(&self.selected).is_none() {
            // The entry that slid into the freed slot, or the new last one when the removed entry was last.
            self.selected = self.items[at.min(self.items.len() - 1)].id().to_owned();
        }
        Some(removed)
    }

    /// Put `id` at `index` of the list as it is afterwards; an index past the end means the end.
    pub fn move_to(&mut self, id: &str, index: usize) -> bool {
        let Some(from) = self.position(id) else { return false };
        let item = self.items.remove(from);
        self.items.insert(index.min(self.items.len()), item);
        true
    }

    /// Rename `id`. An entry always has a name, so a blank one is refused.
    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        if name.trim().is_empty() {
            return false;
        }
        let Some(entry) = self.get_mut(id) else { return false };
        *entry.name_mut() = name.to_owned();
        true
    }
}

#[cfg(test)]
pub(super) mod contract {

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

        assert!(list.select(&start[1]));
        assert_eq!(list.selected(), start[1]);
        assert!(!list.select("missing"));
        assert_eq!(list.selected(), start[1], "an unknown id changes nothing");

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

        assert!(list.rename(&added, "Renamed"));
        assert_eq!(list.name_of(&added).as_deref(), Some("Renamed"));
        assert!(!list.rename(&added, "  "));
        assert!(!list.rename(&added, ""));
        assert!(!list.rename("missing", "Name"));
        assert_eq!(list.name_of(&added).as_deref(), Some("Renamed"), "a blank name is refused");

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

        let removed = [ids[1].clone(), last, drop];
        for _ in 0..20 {
            let id = list.add("New");
            assert!(!removed.contains(&id), "{id} was removed earlier");
        }

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

//! Ordered collections shared by applications.
//!
//! Keys identify entries independently of their positions. The caller defines
//! the payload and key types. Storage and interchange belong to the caller.

pub mod codec;
pub mod links;

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct Entry<K, T> {
    pub key: K,
    pub value: T,
}

#[derive(Debug)]
pub struct Collection<K, T> {
    entries: Vec<Entry<K, T>>,
    positions: BTreeMap<K, usize>,
}

impl<K, T> Default for Collection<K, T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            positions: BTreeMap::new(),
        }
    }
}

impl<K: Clone + Ord, T> Collection<K, T> {
    /// Append in reading order. A duplicate key returns the rejected entry.
    pub fn push(&mut self, entry: Entry<K, T>) -> Result<(), Entry<K, T>> {
        use std::collections::btree_map::Entry as Position;
        match self.positions.entry(entry.key.clone()) {
            Position::Occupied(_) => Err(entry),
            Position::Vacant(position) => {
                position.insert(self.entries.len());
                self.entries.push(entry);
                Ok(())
            }
        }
    }

    pub fn entries(&self) -> &[Entry<K, T>] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<Entry<K, T>> {
        self.entries
    }

    pub fn get(&self, key: &K) -> Option<&T> {
        self.positions
            .get(key)
            .map(|&position| &self.entries[position].value)
    }

    /// Update a payload in place as work completes without changing its order.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut T> {
        let position = *self.positions.get(key)?;
        Some(&mut self.entries[position].value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_order_does_not_change_reading_order() {
        let mut pages = Collection::default();
        pages
            .push(Entry {
                key: 9,
                value: None,
            })
            .unwrap();
        pages
            .push(Entry {
                key: 2,
                value: None,
            })
            .unwrap();
        *pages.get_mut(&2).unwrap() = Some("translated second page");
        assert_eq!(pages.entries()[0].key, 9);
        assert_eq!(pages.entries()[1].key, 2);
        assert_eq!(pages.get(&9), Some(&None));
        assert_eq!(pages.get(&2), Some(&Some("translated second page")));
        assert_eq!(pages.get(&3), None);
    }

    #[test]
    fn duplicate_key_preserves_original_entry() {
        let mut pages = Collection::default();
        pages
            .push(Entry {
                key: 1,
                value: "original",
            })
            .unwrap();
        let duplicate = Entry {
            key: 1,
            value: "duplicate",
        };
        assert_eq!(
            pages.push(duplicate),
            Err(Entry {
                key: 1,
                value: "duplicate"
            })
        );
        assert_eq!(pages.entries().len(), 1);
        assert_eq!(pages.get(&1), Some(&"original"));
    }
}

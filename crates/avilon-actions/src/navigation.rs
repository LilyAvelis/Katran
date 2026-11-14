//! Navigation over a host-supplied visible list; items are stable host IDs.

use super::palette::Direction;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemCursor<T> {
    current: Option<T>,
}

impl<T: Clone + PartialEq> ItemCursor<T> {
    pub fn current(&self) -> Option<&T> {
        self.current.as_ref()
    }
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// A category change invalidates an item absent from the new visible list.
    pub fn reconcile(&mut self, visible: &[T]) {
        if self
            .current
            .as_ref()
            .is_some_and(|id| !visible.contains(id))
        {
            self.clear();
        }
    }

    pub fn first(&mut self, visible: &[T]) -> Option<&T> {
        self.current = visible.first().cloned();
        self.current()
    }

    /// From neutral, Next enters first and Previous enters last.
    pub fn step(&mut self, visible: &[T], direction: Direction) -> Option<&T> {
        if visible.is_empty() {
            self.clear();
            return None;
        }
        let index = self
            .current
            .as_ref()
            .and_then(|id| visible.iter().position(|item| item == id));
        let next = match (index, direction) {
            (None, Direction::Next) => 0,
            (None, Direction::Previous) => visible.len() - 1,
            (Some(index), Direction::Next) => (index + 1) % visible.len(),
            (Some(index), Direction::Previous) => (index + visible.len() - 1) % visible.len(),
        };
        self.current = Some(visible[next].clone());
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_chat_does_not_block_new_category() {
        let mut cursor = ItemCursor::default();
        assert_eq!(cursor.first(&["old"]), Some(&"old"));
        cursor.reconcile(&["new", "other"]);
        assert_eq!(cursor.current(), None);
        assert_eq!(
            cursor.step(&["new", "other"], Direction::Next),
            Some(&"new")
        );
        cursor.clear();
        assert_eq!(cursor.current(), None);
    }

    #[test]
    fn empty_and_wrap_are_defined() {
        let mut cursor = ItemCursor::default();
        assert_eq!(cursor.step(&[1, 2], Direction::Previous), Some(&2));
        assert_eq!(cursor.step(&[1, 2], Direction::Next), Some(&1));
        assert_eq!(cursor.step(&[], Direction::Next), None);
    }
}

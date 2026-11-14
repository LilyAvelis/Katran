//! UI-independent command search and keyboard selection.

/// The host owns display, execution, and persistence.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub group: &'a str,
    pub search_terms: &'a str,
    pub hidden_when_empty: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    query: String,
    selected: usize,
}

impl Palette {
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Changing the query starts selection at the first visible result.
    pub fn set_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.query != query {
            self.query = query;
            self.selected = 0;
        }
    }

    /// Return catalogue indices in source order. Each query word may match any field.
    pub fn visible(&self, entries: &[Entry<'_>]) -> Vec<usize> {
        let words: Vec<_> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if words.is_empty() {
                    return (!entry.hidden_when_empty).then_some(index);
                }
                let haystack = format!("{} {} {}", entry.label, entry.group, entry.search_terms)
                    .to_lowercase();
                words
                    .iter()
                    .all(|word| haystack.contains(word))
                    .then_some(index)
            })
            .collect()
    }

    /// Clamp after catalogue changes, including an empty result set.
    pub fn reconcile(&mut self, visible_count: usize) {
        self.selected = self.selected.min(visible_count.saturating_sub(1));
    }

    /// Move with wraparound. The host scrolls the selected row into view.
    pub fn step(&mut self, visible_count: usize, direction: Direction) {
        if visible_count == 0 {
            self.selected = 0;
            return;
        }
        self.reconcile(visible_count);
        self.selected = match direction {
            Direction::Next => (self.selected + 1) % visible_count,
            Direction::Previous => (self.selected + visible_count - 1) % visible_count,
        };
    }

    pub fn selected_id<'a>(&self, entries: &[Entry<'a>], visible: &[usize]) -> Option<&'a str> {
        visible
            .get(self.selected)
            .and_then(|index| entries.get(*index))
            .map(|entry| entry.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Previous,
    Next,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRIES: [Entry<'static>; 3] = [
        Entry {
            id: "glass",
            label: "Adjust glass",
            group: "Chat",
            search_terms: "bubbles",
            hidden_when_empty: false,
        },
        Entry {
            id: "theme",
            label: "Choose color",
            group: "Chat",
            search_terms: "theme accent",
            hidden_when_empty: false,
        },
        Entry {
            id: "internal",
            label: "Reset color",
            group: "Debug",
            search_terms: "theme",
            hidden_when_empty: true,
        },
    ];

    #[test]
    fn search_and_selection_follow_visible_entries() {
        let mut state = Palette::default();
        assert_eq!(state.visible(&ENTRIES), vec![0, 1]);
        state.step(2, Direction::Previous);
        assert_eq!(
            state.selected_id(&ENTRIES, &state.visible(&ENTRIES)),
            Some("theme")
        );
        state.set_query("CHAT ACCENT");
        let visible = state.visible(&ENTRIES);
        assert_eq!(visible, vec![1]);
        assert_eq!(state.selected_id(&ENTRIES, &visible), Some("theme"));
        state.set_query("debug theme");
        assert_eq!(state.visible(&ENTRIES), vec![2]);
        state.set_query("missing");
        state.step(0, Direction::Next);
        assert_eq!(state.selected_id(&ENTRIES, &state.visible(&ENTRIES)), None);
    }

    #[test]
    fn shrinking_catalogue_clamps_selection() {
        let mut state = Palette::default();
        state.step(3, Direction::Previous);
        state.reconcile(1);
        assert_eq!(state.selected(), 0);
    }
}

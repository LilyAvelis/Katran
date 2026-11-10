use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShelfTab {
    All,
    #[default]
    Images,
    Screenshots,
    Downloads,
    Text,
    Links,
    Tables,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Gallery,
    List,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Replace,
    Toggle,
    Range,
}
#[derive(Default, Serialize)]
pub struct Interaction {
    pub sort: crate::sorting::SortSpec,
    pub activation_serial: u64,
    pub settings_open: bool,
    pub settings_section: usize,
    pub view: View,
    pub tab: ShelfTab,
    pub last_tab: ShelfTab,
    pub visible: Vec<i64>,
    pub cursor: Option<i64>,
    pub selected: Vec<i64>,
    pub preview: Option<i64>,
    pub preview_view: crate::preview_view::State,
    pub table_view: crate::table_view::State,
    anchor: Option<i64>,
    #[serde(skip)]
    range_order: bool,
}
impl Interaction {
    pub fn set_tab(&mut self, tab: ShelfTab) {
        self.tab = tab;
        if tab != ShelfTab::All {
            self.last_tab = tab;
        }
    }
    pub fn toggle_tab(&mut self) {
        self.set_tab(if self.tab == ShelfTab::All {
            self.last_tab
        } else {
            ShelfTab::All
        });
    }
    pub fn set_preview(&mut self, id: Option<i64>) {
        if self.preview != id {
            self.preview_view = Default::default();
            self.table_view = Default::default();
        }
        self.preview = id;
    }
    pub fn visible(&mut self, ids: Vec<i64>) {
        if ids == self.visible {
            return;
        }
        let previous = self
            .cursor
            .and_then(|id| self.visible.iter().position(|i| *i == id))
            .unwrap_or(0);
        self.visible = ids;
        if self.range_order {
            let ranks: std::collections::HashMap<_, _> = self
                .visible
                .iter()
                .enumerate()
                .map(|(rank, id)| (*id, rank))
                .collect();
            self.selected
                .sort_by_key(|id| ranks.get(id).copied().unwrap_or(usize::MAX));
        }
        if self.preview.is_some_and(|id| !self.visible.contains(&id)) {
            self.set_preview(None);
        }
        if !self.cursor.is_some_and(|id| self.visible.contains(&id)) {
            self.cursor = self
                .visible
                .get(previous.min(self.visible.len().saturating_sub(1)))
                .copied();
        }
        if self.anchor.is_some_and(|id| !self.visible.contains(&id)) {
            self.anchor = self.cursor;
        }
    }
    pub fn select(&mut self, id: i64, mode: SelectionMode) {
        let Some(end) = self.visible.iter().position(|i| *i == id) else {
            return;
        };
        match mode {
            SelectionMode::Replace => {
                self.range_order = false;
                self.selected = vec![id];
                self.anchor = Some(id);
            }
            SelectionMode::Toggle => {
                self.range_order = false;
                if self.selected.contains(&id) {
                    self.selected.retain(|i| *i != id);
                } else {
                    self.selected.push(id);
                }
                self.anchor = Some(id);
            }
            SelectionMode::Range => {
                self.range_order = true;
                let anchor = self.anchor.or(self.cursor).unwrap_or(id);
                self.anchor = Some(anchor);
                let start = self
                    .visible
                    .iter()
                    .position(|i| *i == anchor)
                    .unwrap_or(end);
                self.selected = self.visible[start.min(end)..=start.max(end)].to_vec();
            }
        }
        self.cursor = Some(id);
    }
    pub fn navigate(&mut self, delta: i32, mode: SelectionMode) {
        if self.visible.is_empty() {
            return;
        }
        let current = self
            .cursor
            .and_then(|id| self.visible.iter().position(|i| *i == id))
            .unwrap_or(0);
        let index =
            (current as i64 + delta as i64).clamp(0, self.visible.len() as i64 - 1) as usize;
        self.select(self.visible[index], mode);
    }
}

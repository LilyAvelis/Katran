use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShelfTab {
    #[serde(rename = "folder", alias = "all")]
    All,
    #[default]
    Images,
    Screenshots,
    Downloads,
    Text,
    Links,
    Tables,
    Pods,
    Files,
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
    pub pinned_only: bool,
    pub tag_filter: Option<String>,
    pub tag_slot_filter: Option<u8>,
    pub view_generation: u64,
    pub folder_path: Option<std::path::PathBuf>,
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
        if self.tab != tab
            || self.pinned_only
            || self.tag_filter.is_some()
            || self.tag_slot_filter.is_some()
        {
            self.reset_context();
        }
        self.tab = tab;
        if tab != ShelfTab::All {
            self.last_tab = tab;
        }
    }
    pub fn reset_context(&mut self) {
        self.pinned_only = false;
        self.tag_filter = None;
        self.tag_slot_filter = None;
        self.view_generation = self.view_generation.wrapping_add(1);
        self.visible.clear();
        self.selected.clear();
        self.cursor = None;
        self.anchor = None;
        self.range_order = false;
        self.set_preview(None);
    }
    pub fn select_position(&mut self, tab: ShelfTab) {
        if self.tab == tab && tab != ShelfTab::All {
            self.toggle_pinned_only();
        } else {
            self.set_tab(tab);
        }
    }
    pub fn toggle_pinned_only(&mut self) {
        let pinned_only = !self.pinned_only;
        let tag_filter = self.tag_filter.clone();
        let tag_slot_filter = self.tag_slot_filter;
        self.reset_context();
        self.pinned_only = pinned_only;
        self.tag_filter = tag_filter;
        self.tag_slot_filter = tag_slot_filter;
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
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.range_order = false;
        self.anchor = self.cursor;
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

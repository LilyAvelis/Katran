//! Persistent ordering of numbered tabs. All always occupies position zero.
use crate::{api, interaction::ShelfTab};

pub fn default_order() -> Vec<ShelfTab> {
    vec![ShelfTab::Images, ShelfTab::Screenshots, ShelfTab::Downloads]
}

pub fn validate_order(order: &[ShelfTab]) -> Result<(), String> {
    let expected = default_order();
    if order.len() != expected.len()
        || expected
            .iter()
            .any(|tab| order.iter().filter(|item| *item == tab).count() != 1)
    {
        return Err(
            "Tab order must contain Images, Screenshots and Downloads exactly once; All is fixed"
                .into(),
        );
    }
    Ok(())
}

/// Move a numbered tab to a zero-based index, preserving all other relative order.
/// Invalid requests leave the order unchanged.
pub fn reorder(order: &mut Vec<ShelfTab>, tab: ShelfTab, index: usize) -> Result<(), String> {
    validate_order(order)?;
    if index >= order.len() {
        return Err("Tab index is outside the numbered tabs".into());
    }
    let from = order
        .iter()
        .position(|item| *item == tab)
        .ok_or_else(|| "All is fixed and cannot be reordered".to_string())?;
    let moved = order.remove(from);
    order.insert(index, moved);
    Ok(())
}

/// Resolve physical shortcut positions: zero is All, one through nine are numbered tabs.
pub fn tab_at(order: &[ShelfTab], position: u8) -> api::Result<ShelfTab> {
    validate_order(order).map_err(|message| api::ApiError::new("invalid_tab_order", message))?;
    if position == 0 {
        return Ok(ShelfTab::All);
    }
    if position <= 9
        && let Some(tab) = order.get(usize::from(position - 1))
    {
        return Ok(*tab);
    }
    Err(api::ApiError::new(
        "invalid_tab_position",
        "No tab occupies this shortcut position",
    ))
}

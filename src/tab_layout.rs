//! Persistent ordering of numbered tabs. All always occupies position zero.
use crate::{api, interaction::ShelfTab};

pub fn default_order() -> Vec<ShelfTab> {
    vec![
        ShelfTab::Images,
        ShelfTab::Screenshots,
        ShelfTab::Downloads,
        ShelfTab::Text,
        ShelfTab::Links,
        ShelfTab::Tables,
    ]
}

pub fn deserialize_order<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ShelfTab>, D::Error> {
    let mut order = <Vec<ShelfTab> as serde::Deserialize>::deserialize(deserializer)?;
    if order.len() == 3 && !order.contains(&ShelfTab::Text) {
        order.push(ShelfTab::Text);
    }
    if order.len() == 4 && !order.contains(&ShelfTab::Links) {
        order.push(ShelfTab::Links);
    }
    if order.len() == 5 && !order.contains(&ShelfTab::Tables) {
        order.push(ShelfTab::Tables);
    }
    Ok(order)
}

pub fn validate_order(order: &[ShelfTab]) -> Result<(), String> {
    let expected = default_order();
    if order.len() != expected.len()
        || expected
            .iter()
            .any(|tab| order.iter().filter(|item| *item == tab).count() != 1)
    {
        return Err("Tab order must contain every numbered tab exactly once; All is fixed".into());
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

/// Validate a subset of numbered tabs, with no duplicates or All.
pub fn validate_visible(tabs: &[ShelfTab]) -> Result<(), String> {
    let allowed = default_order();
    if tabs
        .iter()
        .any(|tab| !allowed.contains(tab) || tabs.iter().filter(|item| *item == tab).count() != 1)
    {
        return Err("Tabs must contain only distinct numbered tabs; All is fixed".into());
    }
    Ok(())
}

/// Resolve shortcut positions in an already filtered list of visible tabs.
pub fn tab_at_visible(order: &[ShelfTab], position: u8) -> api::Result<ShelfTab> {
    validate_visible(order).map_err(|message| api::ApiError::new("invalid_tab_order", message))?;
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

//! Reusable layout of non-selectable category dividers and selectable card rows.
use serde_json::Value;
use std::ops::Range;

#[derive(Debug, PartialEq)]
pub(super) enum Kind {
    Divider { level: &'static str, label: String },
    Cards(Range<usize>),
}
#[derive(Debug)]
pub(super) struct Row {
    pub top: f32,
    pub height: f32,
    pub kind: Kind,
}
pub(super) struct Layout {
    pub rows: Vec<Row>,
    pub height: f32,
}

pub(super) fn layout(
    events: &[Value],
    columns: usize,
    card_height: f32,
    gap: f32,
    grouped: bool,
) -> Layout {
    let mut rows = Vec::new();
    let mut top = 0.0;
    let mut group = "";
    let mut subgroup = "";
    let mut start = 0;
    let mut i = 0;
    let columns = columns.max(1);
    let push = |kind, height, rows: &mut Vec<Row>, top: &mut f32| {
        if !rows.is_empty() {
            *top += gap;
        }
        rows.push(Row {
            top: *top,
            height,
            kind,
        });
        *top += height;
    };
    while i < events.len() {
        let next_group = if grouped {
            events[i]["category"]["group"].as_str().unwrap_or("")
        } else {
            ""
        };
        let next_subgroup = if grouped {
            events[i]["category"]["subgroup"].as_str().unwrap_or("")
        } else {
            ""
        };
        if grouped && (next_group != group || next_subgroup != subgroup) {
            if start < i {
                push(Kind::Cards(start..i), card_height, &mut rows, &mut top);
            }
            if next_group != group {
                push(
                    Kind::Divider {
                        level: "group",
                        label: next_group.into(),
                    },
                    30.0,
                    &mut rows,
                    &mut top,
                );
            }
            if !next_subgroup.is_empty() {
                push(
                    Kind::Divider {
                        level: "subgroup",
                        label: next_subgroup.into(),
                    },
                    24.0,
                    &mut rows,
                    &mut top,
                );
            }
            group = next_group;
            subgroup = next_subgroup;
            start = i;
        }
        i += 1;
        if i - start == columns {
            push(Kind::Cards(start..i), card_height, &mut rows, &mut top);
            start = i;
        }
    }
    if start < i {
        push(Kind::Cards(start..i), card_height, &mut rows, &mut top);
    }
    Layout { rows, height: top }
}

/// Move between card rows while skipping divider rows and preserving the nearest column.
pub(super) fn vertical_target(
    events: &[Value],
    columns: usize,
    index: usize,
    down: bool,
) -> Option<usize> {
    let layout = layout(events, columns, 1.0, 0.0, true);
    let cards: Vec<_> = layout
        .rows
        .iter()
        .filter_map(|row| match &row.kind {
            Kind::Cards(range) => Some(range),
            _ => None,
        })
        .collect();
    let row = cards.iter().position(|range| range.contains(&index))?;
    let next = if down {
        (row + 1 < cards.len()).then_some(row + 1)
    } else {
        row.checked_sub(1)
    }?;
    let col = index - cards[row].start;
    Some((cards[next].start + col).min(cards[next].end - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn divider_rows_do_not_consume_card_indices() {
        let items = vec![
            serde_json::json!({"category":{"group":"A","subgroup":"one"}}),
            serde_json::json!({"category":{"group":"A","subgroup":"two"}}),
            serde_json::json!({"category":{"group":"A","subgroup":"two"}}),
        ];
        let layout = layout(&items, 3, 80.0, 4.0, true);
        let cards: Vec<_> = layout
            .rows
            .iter()
            .filter_map(|r| {
                if let Kind::Cards(range) = &r.kind {
                    Some(range.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(cards, vec![0..1, 1..3]);
        assert_eq!(vertical_target(&items, 3, 0, true), Some(1));
        assert_eq!(vertical_target(&items, 3, 2, false), Some(0));
        assert_eq!(
            layout
                .rows
                .iter()
                .filter(|r| matches!(r.kind, Kind::Divider { .. }))
                .count(),
            3
        );
        assert!(
            layout
                .rows
                .windows(2)
                .all(|r| r[1].top >= r[0].top + r[0].height)
        );
    }

    #[test]
    fn group_without_subgroup_has_no_empty_divider() {
        let items = vec![serde_json::json!({"category":{"group":"A","subgroup":""}})];
        let layout = layout(&items, 3, 80.0, 4.0, true);
        assert_eq!(layout.rows.len(), 2);
        assert!(
            matches!(&layout.rows[0].kind, Kind::Divider { level: "group", label } if label == "A")
        );
        assert!(matches!(&layout.rows[1].kind, Kind::Cards(range) if range == &(0..1)));
    }
}

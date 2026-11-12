//! Pure geometry for compact, square-card grids.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactLayout {
    pub columns: usize,
    pub tile_width: f32,
    /// Equal space on each side of a complete row, within the supplied width.
    pub inset: f32,
}

/// Fit the requested number of columns, reducing it only when tiles would be
/// smaller than `min_tile`. A single tile shrinks when even it cannot fit.
/// `max_tile` may be infinite to fill the available width without a size cap.
/// The caller should pass the actual scroll area's inner width, not its outer
/// container width. Partial rows retain the same column positions as full rows.
pub fn compact_layout(
    width: f32,
    desired_columns: usize,
    gap: f32,
    min_tile: f32,
    max_tile: f32,
) -> CompactLayout {
    let width = if width.is_finite() {
        width.max(0.0)
    } else {
        0.0
    } as f64;
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 } as f64;
    let min_tile = if min_tile.is_finite() {
        min_tile.max(1.0)
    } else {
        1.0
    } as f64;
    let max_tile = if max_tile.is_nan() {
        min_tile
    } else {
        (max_tile as f64).max(min_tile)
    };
    let capacity = (((width + gap) / (min_tile + gap)).floor() as usize).max(1);
    let columns = desired_columns.max(1).min(capacity);
    let free = (width - gap * (columns - 1) as f64).max(0.0);
    let tile_width = (free / columns as f64).min(max_tile);
    let inset =
        ((width - (tile_width * columns as f64 + gap * (columns - 1) as f64)) / 2.0).max(0.0);
    CompactLayout {
        columns,
        tile_width: tile_width as f32,
        inset: inset as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(width: f32, count: usize, max: f32) -> CompactLayout {
        let g = compact_layout(width, count, 12.0, 48.0, max);
        let used = 2.0 * g.inset + g.tile_width * g.columns as f32 + 12.0 * (g.columns - 1) as f32;
        assert!((used - width).abs() < 0.001, "{g:?}");
        g
    }

    #[test]
    fn requested_counts_fill_width_with_equal_margins() {
        for width in [320.0, 715.0, 960.0, 1920.0] {
            for columns in 1..=24 {
                let g = check(width, columns, f32::INFINITY);
                assert!(g.tile_width >= 48.0);
                assert!(g.inset < 0.001);
                assert!(g.columns <= columns);
            }
        }
    }

    #[test]
    fn max_size_centers_the_complete_grid() {
        let g = check(1000.0, 3, 192.0);
        assert_eq!(g.columns, 3);
        assert_eq!(g.tile_width, 192.0);
        assert_eq!(g.inset, 200.0);
    }

    #[test]
    fn narrow_width_and_bad_inputs_remain_bounded() {
        let g = compact_layout(20.0, 50, 12.0, 48.0, f32::INFINITY);
        assert_eq!(g.columns, 1);
        assert_eq!(g.tile_width, 20.0);
        assert_eq!(g.inset, 0.0);
        let g = compact_layout(f32::NAN, 0, f32::NAN, f32::NAN, f32::NAN);
        assert_eq!(
            g,
            CompactLayout {
                columns: 1,
                tile_width: 0.0,
                inset: 0.0
            }
        );
    }
}

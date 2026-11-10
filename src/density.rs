//! Discrete shelf density shared by all clients.

pub const MIN_DENSITY: u8 = 1;
pub const MAX_DENSITY: u8 = 12;
pub const DEFAULT_GALLERY_COLUMNS: u8 = 4;
pub const DEFAULT_STREAM_ROWS: u8 = 3;

/// Positive steps show more items; negative steps show fewer.
pub fn adjust_density(current: u8, delta: i8) -> u8 {
    (i16::from(current) + i16::from(delta)).clamp(i16::from(MIN_DENSITY), i16::from(MAX_DENSITY))
        as u8
}

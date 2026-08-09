//! Shared euclid geometry and the built-in resize choices.

use euclid::default::Size2D;

/// Serde adapter preserving the TOML [width, height] representation.
pub mod optional_window_size_serde;

/// A window client-area size in physical pixels.
pub type WindowSize = Size2D<i32>;

/// Named groups of built-in resize choices shown by the application.
pub const RESOLUTION_GROUPS: &[(&str, &[WindowSize])] = &[("16:10", RESOLUTIONS_16_10)];

/// Built-in 16:10 client-area sizes, ordered from largest to smallest.
pub const RESOLUTIONS_16_10: &[WindowSize] = &[
    Size2D::new(3840, 2400),
    Size2D::new(2880, 1800),
    Size2D::new(2560, 1600),
    Size2D::new(1920, 1200),
    Size2D::new(1680, 1050),
    Size2D::new(1440, 900),
    Size2D::new(1280, 800),
    Size2D::new(960, 600),
    Size2D::new(800, 500),
    Size2D::new(640, 400),
    Size2D::new(480, 300),
];

/// Returns whether `size` is one of the built-in resize choices.
pub fn is_known_resolution(size: WindowSize) -> bool {
    RESOLUTION_GROUPS
        .iter()
        .flat_map(|&(_, resolutions)| resolutions)
        .any(|candidate| *candidate == size)
}

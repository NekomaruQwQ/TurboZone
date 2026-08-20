//! Shared euclid geometry and the built-in resize choices.

use euclid::default::Size2D;

/// Standard client-area sizes, ordered from largest to smallest.
pub const WINDOW_SIZE_MANIFEST: &[(&str, &[Size2D<i32>])] = {
    const fn into_size2d(size: (i32, i32)) -> Size2D<i32> {
        Size2D::new(size.0, size.1)
    }

    &[
        ("16:10", &[
            (3840, 2400),
            (2880, 1800),
            (2560, 1600),
            (1920, 1200),
            (1680, 1050),
            (1440, 900),
            (1280, 800),
            (960, 600),
        ].map(into_size2d)),
        ("16:9", &[
            (3840, 2160),
            (2880, 1620),
            (2560, 1440),
            (1920, 1080),
            (1600, 900),
            (1360, 768),
            (1280, 720),
            (1024, 576),
            (960, 540),
        ].map(into_size2d)),
    ]
};

/// Returns whether a size appears in the built-in selector manifest.
pub fn is_known_window_size(size: Size2D<i32>) -> bool {
    WINDOW_SIZE_MANIFEST.iter()
        .any(|&(_, sizes)| sizes.contains(&size))
}

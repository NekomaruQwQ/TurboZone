pub const APP_NAME: &str = "TurboZone";
pub const APP_WINDOW_SIZE: [f32; 2] = [450.0, 720.0];

/// Native snapshots per second when no user action requests an earlier tick.
pub const LOGIC_TICKS_PER_SECOND: u32 = 10;

/// UI repaints per second while the application is visible.
pub const RENDER_FRAMES_PER_SECOND: u32 = 30;

/// Standard client-area sizes, ordered from largest to smallest.
pub const STANDARD_SIZE: &[(&str, &[[i32; 2]])] = &[
    ("16:10", &[
        [3840, 2400],
        [2880, 1800],
        [2560, 1600],
        [1920, 1200],
        [1680, 1050],
        [1440, 900],
        [1280, 800],
        [960, 600],
    ]),
    ("16:9", &[
        [3840, 2160],
        [2880, 1620],
        [2560, 1440],
        [1920, 1080],
        [1600, 900],
        [1360, 768],
        [1280, 720],
        [1024, 576],
        [960, 540],
    ]),
];

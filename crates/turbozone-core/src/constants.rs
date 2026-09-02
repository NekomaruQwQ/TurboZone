pub const APP_NAME: &str = "TurboZone";
pub const APP_WINDOW_SIZE: [f32; 2] = [450.0, 720.0];

/// Native snapshots per second when no user action requests an earlier tick.
pub const LOGIC_TICKS_PER_SECOND: u32 = 10;

/// UI repaints per second while the application is visible.
pub const RENDER_FRAMES_PER_SECOND: u32 = 30;

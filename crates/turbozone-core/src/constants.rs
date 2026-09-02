//! Product cadence constants shared with framework adapters.

/// Native snapshots per second when no user action requests an earlier tick.
pub const LOGIC_TICKS_PER_SECOND: u32 = 10;

/// UI repaints per second while the application is visible.
pub const RENDER_FRAMES_PER_SECOND: u32 = 30;

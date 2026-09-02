//! Backend-independent action orchestration and snapshot lifecycle.

use std::fmt;
use std::hash::Hash;

use euclid::default::Size2D;

use crate::{SnapshotLogging, RuntimeConfig, WindowInfo, WindowSection, group_windows};

/// One native side effect accepted from a rendered snapshot.
///
/// Actions own exactly one handle so queues, ordering, and per-target failures remain
/// explicit. New variants may be added without allowing presentation crates to assume
/// they know the complete backend operation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowAction<H> {
    /// Sets the client area to an exact positive physical-pixel size.
    Resize(H, Size2D<i32>),
    /// Centers the live or restored client area in its current monitor work area.
    MoveToCenter(H),
}

impl<H: Copy> WindowAction<H> {
    /// Returns the native identity captured when the action was accepted.
    pub const fn handle(&self) -> H {
        match *self {
            Self::Resize(handle, _) | Self::MoveToCenter(handle) => handle,
        }
    }
}

/// Supplies snapshots and interprets native actions for one platform.
///
/// Core forwards [`WindowAction`] values without dispatching their variants. This keeps
/// platform-specific dispatch beside the native mechanisms while core retains queue
/// order, refresh policy, and non-fatal error handling.
pub trait Backend {
    /// Cheap, stable identity retained between a snapshot and its deferred action.
    ///
    /// Its [`fmt::Display`] implementation is used for logging and error messages,
    /// so it should provide a human-readable representation of the native window
    /// identity as much as possible.
    type Handle:
        fmt::Debug +
        fmt::Display +
        Copy + PartialEq + Eq + Hash + 'static;

    /// Captures the currently relevant application windows.
    fn snapshot(&mut self) -> anyhow::Result<Vec<WindowInfo<Self::Handle>>>;

    /// Performs one action against live native state.
    ///
    /// Implementations return operational failures and must panic for an unsupported
    /// future action variant rather than silently accepting an operation they did not
    /// perform.
    fn perform(&mut self, action: WindowAction<Self::Handle>) -> anyhow::Result<()>;
}

/// Owns product state and advances it only through explicit logic ticks.
///
/// Presentation code may queue work and inspect the latest sections, but only a tick
/// performs native effects or replaces the snapshot. Actions run before refresh so the
/// resulting snapshot reflects accepted user operations.
pub struct Engine<B: Backend> {
    backend: B,
    config: RuntimeConfig,
    sections: Vec<WindowSection<B::Handle>>,
    pending_actions: Vec<WindowAction<B::Handle>>,
    logging: SnapshotLogging<B::Handle>,
}

impl<B: Backend> Engine<B> {
    /// Creates an unticked engine with no stale native state.
    pub fn new(config: RuntimeConfig, backend: B) -> Self {
        Self {
            backend,
            config,
            sections: Vec::new(),
            pending_actions: Vec::new(),
            logging: SnapshotLogging::default(),
        }
    }

    /// Returns the active validated configuration.
    pub const fn config(&self) -> &RuntimeConfig { &self.config }

    /// Returns the latest successfully grouped snapshot.
    pub fn sections(&self) -> &[WindowSection<B::Handle>] { &self.sections }

    /// Defers one native operation until the next logic tick.
    pub fn queue(&mut self, action: WindowAction<B::Handle>) { self.pending_actions.push(action); }

    /// Returns whether user work should trigger a tick before the periodic deadline.
    pub const fn has_pending_actions(&self) -> bool { !self.pending_actions.is_empty() }

    /// Applies queued operations and refreshes the complete derived view state.
    ///
    /// Individual action and snapshot failures are logged as non-fatal. A failed
    /// top-level snapshot clears sections so the UI never presents older data as live.
    pub fn tick(&mut self) {
        for action in std::mem::take(&mut self.pending_actions) {
            let handle = action.handle();
            let identity = self.window_identity(handle);
            if let Err(error) = self.backend.perform(action) {
                log::error!("window action failed for {identity}: {error:#}");
            }
        }

        let windows = match self.backend.snapshot() {
            Ok(windows) => windows,
            Err(error) => {
                self.logging.enumeration_failed(error.to_string());
                self.sections.clear();
                return;
            },
        };
        self.logging.update(&windows);
        self.sections = group_windows(&self.config, windows);
    }

    /// Returns the backend after consuming the engine, primarily for adapter tests.
    pub fn into_backend(self) -> B { self.backend }

    /// Formats cached metadata before the backend consumes the action.
    fn window_identity(&self, handle: B::Handle) -> String {
        let window = self.sections.iter()
            .flat_map(|section| &section.windows)
            .find(|window| window.handle == handle);
        match window {
            Some(window) => if let Ok(detail) = window.detail.as_ref() {
                format!(
                    "{handle:?} title={:?} executable={:?}",
                    window.title,
                    detail.program_path)
            } else {
                format!("{handle:?} title={:?}", window.title)
            },
            None => format!("{handle:?}"),
        }
    }
}

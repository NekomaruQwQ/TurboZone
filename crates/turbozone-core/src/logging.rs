//! Deduplication for non-fatal failures emitted during periodic snapshots.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use smol_str::SmolStr;

use crate::WindowInfo;

/// Remembers only current failures so frequent refreshes do not flood stderr.
///
/// Recovery removes the remembered failure, allowing an identical later recurrence to
/// be reported. The logger stores no presentation state and does not install a global
/// logging implementation.
pub struct SnapshotLogging<H> {
    window_errors: HashMap<H, SmolStr>,
    enumeration_error: Option<SmolStr>,
}

impl<H> Default for SnapshotLogging<H> {
    fn default() -> Self {
        Self { window_errors: HashMap::new(), enumeration_error: None }
    }
}

impl<H: Copy + Debug + Eq + Hash> SnapshotLogging<H> {
    /// Reports changed per-window failures and forgets recovered or absent windows.
    pub fn update(&mut self, windows: &[WindowInfo<H>]) {
        self.enumeration_error = None;
        let mut current = HashMap::new();
        for window in windows {
            if let Err(ref error) = window.detail {
                let message = SmolStr::new(format!("{error:#}"));
                if self.window_errors.get(&window.handle) != Some(&message) {
                    log::warn!(
                        "window {:?} title={:?}: {message}",
                        window.handle,
                        window.title);
                }
                current.insert(window.handle, message);
            }
        }
        self.window_errors = current;
    }

    /// Reports a changed enumeration failure without treating missing data as recovery.
    pub fn enumeration_failed(&mut self, message: impl Into<SmolStr>) {
        let message = message.into();
        if self.enumeration_error.as_ref() != Some(&message) {
            log::error!("window enumeration failed: {message}");
        }
        self.enumeration_error = Some(message);
    }
}

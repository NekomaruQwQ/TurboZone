//! Native observation and action boundary used by the application engine.

use std::fmt;
use std::hash::Hash;

use crate::{WindowAction, WindowSnapshotCollection};

/// Supplies snapshots and interprets native actions for one platform.
///
/// Platform adapters own native reads, writes, filtering, and action dispatch. The engine
/// owns orchestration and derives views; UI code uses only the Engine contract.
pub trait Platform {
    /// Cheap native identity, stable independently of changing display metadata.
    ///
    /// Display provides a best-effort description even when capture failed or the
    /// window disappeared. It must not fail merely because metadata is unavailable.
    type Window: fmt::Debug + fmt::Display + Copy + Eq + Hash + 'static;

    /// Captures relevant windows and their failures in backend enumeration order.
    ///
    /// Each included handle occurs once and equals WindowInfo.handle on success.
    /// Excluded windows are omitted; unmatched successes remain for derived views.
    /// Required-query failures retain the handle beside the original error, while an
    /// outer failure invalidates the entire enumeration. Order may change between calls.
    fn snapshot(&mut self)
     -> anyhow::Result<WindowSnapshotCollection<Self::Window>>;

    /// Performs one action against live state, retaining contextual native failures.
    /// Implementations must panic for an unsupported future action variant.
    fn perform(&mut self, target: Self::Window, action: WindowAction)
     -> anyhow::Result<()>;
}


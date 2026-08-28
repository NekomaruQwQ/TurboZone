//! Stderr logging and suppression of unchanged errors from periodic native queries.

use std::collections::BTreeMap;

use turbozone_core::WindowInfo;
use turbozone_windows::WindowHandle;

/// Installs stderr logging with visible application startup messages and warnings.
/// Explicit `RUST_LOG` filters override the defaults. Returns an error if already installed.
pub fn init_logging() -> Result<(), log::SetLoggerError> {
    let mut logger = pretty_env_logger::formatted_builder();
    logger.target(pretty_env_logger::env_logger::Target::Stderr);
    if let Ok(filters) = std::env::var("RUST_LOG") {
        logger.parse_filters(&filters);
    } else {
        logger.filter_level(log::LevelFilter::Warn)
            .filter_module("turbozone", log::LevelFilter::Info);
    }
    logger.try_init()
}

/// Remembers only current failures so 100 ms refreshes do not flood the terminal.
#[derive(Default)]
pub struct SnapshotDiagnostics {
    /// Last reported failures, keyed by native window identity rather than private titles.
    window_errors: BTreeMap<usize, String>,
    /// Repeated enumeration failures cannot be compared with per-window snapshots.
    enumeration_error: Option<String>,
}

impl SnapshotDiagnostics {
    /// Reports new or changed failures, forgetting windows that recovered or disappeared.
    /// A successful enumeration also ends the preceding enumeration-failure episode.
    pub fn update(&mut self, windows: &[WindowInfo<WindowHandle>]) {
        self.enumeration_error = None;
        let mut current = BTreeMap::new();
        for window in windows {
            if let Err(ref error) = window.detail {
                let address = window.handle.address();
                let message = format!("{error:#}");
                if self.window_errors.get(&address) != Some(&message) {
                    log::warn!("window 0x{address:x}: {message}");
                }
                current.insert(address, message);
            }
        }
        self.window_errors = current;
    }

    /// Reports a changed enumeration failure without mistaking missing data for recovery.
    pub fn enumeration_failed(&mut self, message: String) {
        if self.enumeration_error.as_ref() != Some(&message) {
            log::error!("window enumeration failed: {message}");
        }
        self.enumeration_error = Some(message);
    }
}

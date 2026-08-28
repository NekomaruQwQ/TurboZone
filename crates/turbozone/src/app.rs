use std::time::{Duration, Instant};

use eframe::egui::{Context, Ui};
use euclid::default::Size2D;
use turbozone_core::RuntimeConfig;
use turbozone_windows::{WindowHandle, WindowEnumerator};

use crate::data::{WindowSection, group_windows};
use crate::diagnostics::SnapshotDiagnostics;
use crate::ui;

/// Keeps painting independent from native query frequency.
const RENDER_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
/// Refreshes native details often enough to reflect external window changes.
const LOGIC_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
/// A native side effect accepted by the UI and deferred to the next logic tick.
pub enum Action {
    /// Centers the exact window snapshot visible when the action was accepted.
    Center {
        /// Native handles captured by the UI.
        windows: Vec<WindowHandle>,
    },
    /// Resizes the exact window snapshot visible when the action was accepted.
    Resize {
        /// Native handles captured by the UI.
        windows: Vec<WindowHandle>,
        /// One-shot client-area target.
        size: Size2D<i32>,
    },
}

/// TurboZone application state shared by the logic and UI phases.
pub struct App {
    /// Usable rules compiled once during startup.
    config: RuntimeConfig,
    /// Native enumeration and per-refresh monitor cache.
    snapshotter: WindowEnumerator,
    /// Only matched windows participate in rendering and actions.
    windows: Vec<WindowSection>,
    /// Actions target the handles captured when the user accepted them.
    pending_actions: Vec<Action>,
    /// Suppresses unchanged periodic errors, without storing UI diagnostics.
    diagnostics: SnapshotDiagnostics,
    /// Earliest scheduled native refresh, unless an action is pending.
    next_logic_tick: Instant,
}

impl App {
    /// Accepts compiled rules without performing filesystem or native work.
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            snapshotter: WindowEnumerator::default(),
            windows: Vec::new(),
            pending_actions: Vec::new(),
            diagnostics: SnapshotDiagnostics::default(),
            next_logic_tick: Instant::now(),
        }
    }

    /// Applies queued actions before capturing the resulting window state.
    fn logic_tick(&mut self) {
        for action in std::mem::take(&mut self.pending_actions) {
            apply_action(action);
        }
        self.refresh_windows();
    }

    /// Reports native failures and replaces stale sections on every refresh attempt.
    fn refresh_windows(&mut self) {
        let windows = match self.snapshotter.snapshot() {
            Ok(windows) => windows,
            Err(error) => {
                self.diagnostics.enumeration_failed(error.to_string());
                self.windows.clear();
                return;
            },
        };
        self.diagnostics.update(&windows);
        self.windows = group_windows(&self.config, windows);
    }

    /// Renders controls without performing native actions inside the UI pass.
    fn app_ui(&mut self, ui: &mut Ui) {
        ui::app_ui(
            ui,
            &self.windows,
            &self.config,
            &mut self.pending_actions);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        let now = Instant::now();
        if !self.pending_actions.is_empty() || now >= self.next_logic_tick {
            self.logic_tick();
            self.next_logic_tick = Instant::now()
                .checked_add(LOGIC_INTERVAL)
                .expect("logic interval must fit within Instant");
        }
        ctx.request_repaint_after(
            self.next_logic_tick.saturating_duration_since(Instant::now()));
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        self.app_ui(ui);
        ui.request_repaint_after(RENDER_INTERVAL);
    }
}

/// Reports each failed user-requested action without aborting the remaining targets.
fn apply_action(action: Action) {
    match action {
        Action::Center { windows } => {
            for handle in windows {
                if let Err(error) = turbozone_windows::center_window(handle) {
                    log::error!(
                        "failed to center window 0x{:x}: {error}",
                        handle.address());
                }
            }
        },
        Action::Resize { windows, size } => {
            for handle in windows {
                if let Err(error) = turbozone_windows::resize_window(handle, size) {
                    log::error!(
                        "failed to resize window 0x{:x}: {error}",
                        handle.address());
                }
            }
        },
    }
}

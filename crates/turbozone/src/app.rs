use std::time::{Duration, Instant};

use eframe::egui::{Context, Ui};
use turbozone_core::Size2D;
use turbozone_windows::{WindowHandle, WindowSnapshotter};

use crate::configuration::ConfigState;
use crate::data::{SectionedWindows, WindowPage};
use crate::ui;

const RENDER_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
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
    config: ConfigState,
    snapshotter: WindowSnapshotter,
    windows: SectionedWindows,
    page: WindowPage,
    pending_actions: Vec<Action>,
    native_error: Option<String>,
    next_logic_tick: Instant,
}

impl App {
    /// Loads configuration and creates an initially empty native snapshot.
    pub fn new() -> Self {
        Self {
            config: ConfigState::load(),
            snapshotter: WindowSnapshotter::default(),
            windows: SectionedWindows::default(),
            page: WindowPage::default(),
            pending_actions: Vec::new(),
            native_error: None,
            next_logic_tick: Instant::now(),
        }
    }

    fn logic_tick(&mut self) {
        for action in std::mem::take(&mut self.pending_actions) {
            apply_action(action);
        }
        self.refresh_windows();
    }

    fn refresh_windows(&mut self) {
        let windows = match self.snapshotter.snapshot() {
            Ok(windows) => windows,
            Err(error) => {
                let message = format!("window enumeration failed: {error}");
                log::error!("{message}");
                self.native_error = Some(message);
                self.windows = SectionedWindows::default();
                return;
            },
        };
        self.native_error = None;
        self.windows = SectionedWindows::from_windows(&self.config.runtime, windows);
    }

    fn app_ui(&mut self, ui: &mut Ui) {
        ui::app_ui(
            ui,
            &self.windows,
            &self.config,
            self.native_error.as_deref(),
            &mut self.page,
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

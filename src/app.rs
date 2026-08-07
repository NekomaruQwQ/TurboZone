use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::*;
use euclid::default::*;

use itertools::Itertools as _;
use windows::Win32::Foundation::HWND;

use crate::config::*;
use crate::native;
use crate::core::*;

const CHAR_EMPTY: char = '\u{26ab}';
const CHAR_CHECK: char = '\u{2705}';
const CHAR_CROSS: char = '\u{00d7}';
const CHAR_WINDOW: char = '\u{1f5d6}';

/// Maximum idle time between render passes, corresponding to a 30 FPS baseline.
const RENDER_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
/// Maximum idle time between periodic window-data refreshes.
const LOGIC_INTERVAL: Duration = Duration::from_millis(100);

/// A side effect requested by the UI and applied at the start of a logic tick.
///
/// A list of handles captures the exact window snapshot visible when a command
/// was clicked, so newly discovered windows are not modified retroactively.
enum Action {
    Center {
        windows: Vec<HWND>,
    },
    Resize {
        windows: Vec<HWND>,
        size: Size2D<i32>,
    },
    SetResizeEnabled {
        path: PathBuf,
        enabled: bool,
    },
}

pub struct App {
    config: Config,
    window_map: BTreeMap<Option<PathBuf>, Vec<WindowInfo>>,
    executable_map: HashMap<PathBuf, ExecutableInfo>,
    pending_actions: Vec<Action>,
    /// Next periodic window-data refresh deadline.
    next_logic_tick: Instant,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        let now = Instant::now();

        // User actions make a logic tick due immediately. This preserves the
        // action -> refresh -> render invariant without adding click latency.
        if !self.pending_actions.is_empty() || now >= self.next_logic_tick {
            self.logic_tick();
            self.next_logic_tick = Instant::now()
                .checked_add(LOGIC_INTERVAL)
                .expect("logic interval must fit within Instant");
        }

        // Keep data refreshing even while the native window is hidden and
        // eframe therefore skips `App::ui`.
        ctx.request_repaint_after(
            self.next_logic_tick.saturating_duration_since(Instant::now()));
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        CentralPanel::default().show(ui, |ui| self.main_ui(ui));
        ui.request_repaint_after(RENDER_INTERVAL);
    }
}

impl App {
    pub fn new() -> Self {
        let config =
            load_config()
                .inspect_err(|e| log::error!("{e}"))
                .unwrap_or_default()
                .unwrap_or_default();
        Self {
            window_map: BTreeMap::new(),
            executable_map: HashMap::new(),
            pending_actions: Vec::new(),
            next_logic_tick: Instant::now(),
            config,
        }
    }

    /// Applies all accepted UI actions before publishing a freshly enumerated
    /// window snapshot for the following render pass.
    fn logic_tick(&mut self) {
        for action in std::mem::take(&mut self.pending_actions) {
            self.apply_action(action);
        }
        self.refresh_windows();
    }

    /// Applies one queued side effect, logging target-specific failures so the
    /// remainder of the accepted batch can still run.
    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Center { windows } => {
                for hwnd in windows {
                    center_window(hwnd)
                        .unwrap_or_else(|e| {
                            let address = hwnd.0.addr();
                            log::error!("failed to center window 0x{address:x}: {e}");
                        });
                }
            },
            Action::Resize { windows, size } => {
                for hwnd in windows {
                    resize_window(hwnd, size)
                        .unwrap_or_else(|e| {
                            let address = hwnd.0.addr();
                            log::error!("failed to resize window 0x{address:x}: {e}");
                        });
                }
            },
            Action::SetResizeEnabled { path, enabled } => {
                if enabled {
                    self.config.no_resize.remove(&path);
                } else {
                    self.config.no_resize.insert(path);
                }

                save_config(&self.config)
                    .unwrap_or_else(|e| log::error!("failed to save config: {e}"));
            },
        }
    }

    /// Builds and atomically publishes a window snapshot, preserving the last
    /// successful snapshot when top-level enumeration fails.
    fn refresh_windows(&mut self) {
        let Ok(windows) =
            Self::enumerate_windows()
                .inspect_err(|e| log::error!("enumerate_windows() failed: {e}"))
        else {
            // Preserve the last successful snapshot; an empty replacement would
            // incorrectly imply that every window disappeared simultaneously.
            return;
        };

        let window_map: BTreeMap<_, _> =
            windows
                .into_iter()
                .into_group_map_by(|info| info.executable_path.clone())
                .into_iter()
                .collect();

        // Version-info lookup can touch the filesystem, so populate this cache
        // in the logic phase rather than lazily during rendering.
        for path in window_map.keys().flatten() {
            self.executable_map
                .entry(path.clone())
                .or_insert_with(|| ExecutableInfo::from_path(path));
        }

        self.window_map = window_map;
    }

    /// Enumerates relevant application windows and gathers their display state.
    fn enumerate_windows() -> windows::core::Result<Vec<WindowInfo>> {
        native::enumerate_windows()
            .map(|windows| {
                windows
                    .into_iter()
                    .filter(|&hwnd| is_active(hwnd))
                    .map(WindowInfo::from_hwnd)
                    .filter(|info| !info.window_text.is_empty())
                    .filter(|info| !(
                        info.window_text == "Program Manager" &&
                        info.executable_path
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .map(|name| name.to_string_lossy().to_lowercase())
                            .is_some_and(|name| name == "explorer.exe")))
                    .collect()
            })
    }

    fn main_ui(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (my_path, my_windows) in &self.window_map {
                    let mut group_ui =
                        GroupUI::new(
                            &self.executable_map,
                            &self.config,
                            &mut self.pending_actions,
                            my_path.as_ref(),
                            my_windows);
                    ui.push_id(group_ui.display_path.clone(), |ui| {
                        group_ui.ui(ui);
                    });
                }
            });
    }
}

struct GroupUI<'a> {
    pending_actions: &'a mut Vec<Action>,
    windows: &'a [WindowInfo],
    display_name: String,
    display_path: String,
    normalized_path: Option<PathBuf>,
    resize_enabled: bool,
}

impl<'a> GroupUI<'a> {
    /// Creates a read-only view of a window group plus access to its intent
    /// queue; the latest pending config intent is reflected optimistically.
    fn new(
        executable_map: &'a HashMap<PathBuf, ExecutableInfo>,
        config: &Config,
        pending_actions: &'a mut Vec<Action>,
        path: Option<&'a PathBuf>,
        windows: &'a [WindowInfo]) -> Self {
        let executable_info =
            path.and_then(|path| executable_map.get(path));

        let display_name =
            executable_info
                .and_then(|info| info.display_name.clone())
                .unwrap_or_else(|| "<unknown>".to_owned());
        let display_path =
            executable_info
                .map(|info| info.display_path.clone())
                .unwrap_or_else(|| "<unknown path>".to_owned());

        // Normalize to forward slashes so lookups match the stored form.
        let normalized_path =
            path.map(|p| PathBuf::from(p.to_string_lossy().replace('\\', "/")));
        let resize_enabled =
            normalized_path
                .as_ref()
                .is_none_or(|path| {
                    pending_actions
                        .iter()
                        .rev()
                        .find_map(|action| match action {
                            &Action::SetResizeEnabled {
                                path: ref pending_path,
                                enabled,
                            } if pending_path == path => Some(enabled),
                            _ => None,
                        })
                        .unwrap_or_else(|| !config.no_resize.contains(path))
                });

        Self {
            pending_actions,
            windows,
            display_name,
            display_path,
            normalized_path,
            resize_enabled,
        }
    }

    /// Queues configuration persistence for the next logic phase while keeping
    /// the checkbox's effective state stable until that phase runs.
    fn queue_resize_enabled(&mut self, ui: &Ui, enabled: bool) {
        let Some(ref path) = self.normalized_path else {
            panic!("cannot set resize enabled for unknown executable path");
        };

        self.resize_enabled = enabled;
        queue_action(
            ui,
            self.pending_actions,
            Action::SetResizeEnabled {
                path: path.clone(),
                enabled,
            });
    }
}

impl GroupUI<'_> {
    fn resize_enabled_ui(&mut self, ui: &mut Ui) {
        // Only show the "Resize Enabled" checkbox for groups with a known
        // executable path.
        if self.normalized_path.is_some() {
            let mut enabled = self.resize_enabled;
            if ui.checkbox(&mut enabled, "Resize Enabled").changed() {
                self.queue_resize_enabled(ui, enabled);
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        ui.heading(&self.display_name);
        ui.add(Label::new(RichText::new(&self.display_path).weak()).truncate());
        ui.add_space(4.0);

        // Group-level controls: resize checkbox, center all, resize all.
        ui.horizontal(|ui| {
            // CENTER ALL button.
            if ui.add_sized((80.0, 16.0), Button::new("CENTER ALL")).clicked() {
                queue_action(
                    ui,
                    self.pending_actions,
                    Action::Center {
                        windows: self.windows.iter().map(|window| window.hwnd).collect(),
                    });
            }

            // RESIZE ALL combobox.
            ui.add_enabled_ui(self.resize_enabled, |ui| {
                egui::ComboBox::from_id_salt("resize-all")
                    .width(ui.available_width().min(120.0))
                    .selected_text("Resize All")
                    .show_ui(ui, |ui| {
                        if let Some(size) = resolution_ui(ui, Size2D::zero()) {
                            queue_action(
                                ui,
                                self.pending_actions,
                                Action::Resize {
                                    windows: self.windows.iter().map(|window| window.hwnd).collect(),
                                    size,
                                });
                        }
                    });
            });

            // Checkbox — only shown for groups with a known executable path.
            self.resize_enabled_ui(ui);
        });

        ui.add_space(4.0);

        for window in self.windows {
            ui.push_id(window.hwnd.0, |ui| {
                ui.horizontal(|ui| window_header_ui(ui, window));
                ui.horizontal(|ui| {
                    window_command_ui(
                        ui,
                        window,
                        self.resize_enabled,
                        self.pending_actions);
                });
            });
            ui.add_space(2.0);
        }

        ui.add_space(12.0);
    }
}

fn window_header_ui(ui: &mut Ui, window: &WindowInfo) {
    ui.label(CHAR_WINDOW.to_string());
    match window.state {
        WindowState::Maximized => {
            ui.add(Label::new(RichText::new("[max]").weak()));
        },
        WindowState::Minimized => {
            ui.add(Label::new(RichText::new("[min]").weak()));
        },
        WindowState::Normal => {}
    }
    ui.add(Label::new(&window.window_text).truncate());
}

fn window_command_ui(
    ui: &mut Ui,
    window: &WindowInfo,
    enabled: bool,
    pending_actions: &mut Vec<Action>) {
    if window.is_centered == Some(true) {
        ui.add_sized((80.0, 16.0), Label::new(format!("{CHAR_CHECK}centered")));
    } else {
        let center_clicked =
            ui.add_sized((80.0, 16.0), Button::new("CENTER")).clicked();
        if center_clicked {
            queue_action(
                ui,
                pending_actions,
                Action::Center {
                    windows: vec![window.hwnd],
                });
        }
    }

    ui.add_enabled_ui(enabled, |ui| {
        let size = window.client_size.unwrap_or_default();

        egui::ComboBox::from_id_salt("size")
            .width(ui.available_width().min(120.0))
            .selected_text({
                if size != Size2D::zero() {
                    format!(
                        "{} {}x{}",
                        if is_known_resolution(size) { CHAR_CHECK } else { CHAR_EMPTY },
                        size.width, size.height)
                } else {
                    "<unknown size>".to_owned()
                }
            })
            .show_ui(ui, |ui| {
                if let Some(size) = resolution_ui(ui, size) {
                    queue_action(
                        ui,
                        pending_actions,
                        Action::Resize {
                            windows: vec![window.hwnd],
                            size,
                        });
                }
            });
    });
}

/// Records a UI intent and wakes eframe so `App::logic` can apply it before the
/// next render pass instead of waiting for the periodic logic deadline.
fn queue_action(ui: &Ui, pending_actions: &mut Vec<Action>, action: Action) {
    pending_actions.push(action);
    ui.request_repaint();
}

/// Renders the supported resolutions and returns the one selected this pass.
fn resolution_ui(
    ui: &mut Ui,
    selected: Size2D<u32>) -> Option<Size2D<i32>> {
    let mut selected_resolution = None;

    for &(name, arr) in RESOLUTION_GROUPS {
        ui.add_sized(
            (ui.available_width(), 0.0),
            egui::Label::new(
                egui::RichText::new(format!("-{name}-  ")).weak()));
        for &resolution in arr {
            ui.selectable_value(
                &mut format!("{}x{}", resolution.width, resolution.height),
                format!("{}x{}", selected.width, selected.height),
                format!("{}{}{}", resolution.width, CHAR_CROSS, resolution.height))
                .clicked()
                .then(|| selected_resolution = Some(resolution));
        }
        ui.label("");
    }

    selected_resolution
}


use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use egui::*;
use euclid::default::*;

use itertools::Itertools as _;

use crate::config::*;
use crate::native;
use crate::core::*;

const CHAR_EMPTY: char = '\u{26ab}';
const CHAR_CHECK: char = '\u{2705}';
const CHAR_CROSS: char = '\u{00d7}';
const CHAR_WINDOW: char = '\u{1f5d6}';

pub struct App {
    config: Config,
    window_map: BTreeMap<Option<PathBuf>, Vec<WindowInfo>>,
    executable_map: HashMap<PathBuf, ExecutableInfo>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        CentralPanel::default().show_inside(ui, |ui| self.main_ui(ui));
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
            config,
        }
    }

    fn refresh_windows(&mut self) {
        self.window_map =
            Self::enumerate_windows()
                .into_iter()
                .into_group_map_by(|info| info.executable_path.clone())
                .into_iter()
                .collect();
    }

    fn enumerate_windows() -> Vec<WindowInfo> {
        native::enumerate_windows()
            .inspect_err(|e| log::error!("enumerate_windows() failed: {e}"))
            .unwrap_or_default()
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
    }

    fn main_ui(&mut self, ui: &mut Ui) {
        self.refresh_windows();

        ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (my_path, my_windows) in &self.window_map {
                    let mut group_ui =
                        GroupUI::new(
                            &mut self.executable_map,
                            &mut self.config,
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
    config: &'a mut Config,
    windows: &'a [WindowInfo],
    display_name: String,
    display_path: String,
    normalized_path: Option<PathBuf>,
    resize_enabled: bool,
}

impl<'a> GroupUI<'a> {
    fn new(
        executable_map: &'a mut HashMap<PathBuf, ExecutableInfo>,
        config: &'a mut Config,
        path: Option<&'a PathBuf>,
        windows: &'a [WindowInfo]) -> Self {
        let executable_info =
            path.map(|path| &*{
                executable_map
                    .entry(path.clone())
                    .or_insert_with(|| ExecutableInfo::from_path(path))
            });

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
                .is_none_or(|p| !config.no_resize.contains(p));

        Self {
            config,
            windows,
            display_name,
            display_path,
            normalized_path,
            resize_enabled,
        }
    }

    fn set_resize_enabled(&mut self, enabled: bool) {
        let Some(ref path) = self.normalized_path else {
            panic!("cannot set resize enabled for unknown executable path");
        };

        if enabled {
            self.config.no_resize.remove(path);
        } else {
            self.config.no_resize.insert(path.clone());
        }

        save_config(self.config)
            .unwrap_or_else(|e| log::error!("failed to save config: {e}"));
    }
}

impl GroupUI<'_> {
    fn resize_enabled_ui(&mut self, ui: &mut Ui) {
        // Only show the "Resize Enabled" checkbox for groups with a known
        // executable path.
        if self.normalized_path.is_some() {
            let mut enabled = self.resize_enabled;
            if ui.checkbox(&mut enabled, "Resize Enabled").changed() {
                self.set_resize_enabled(enabled);
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
            ui.add_sized((80.0, 16.0), Button::new("CENTER ALL"))
                .clicked()
                .then(|| {
                    for window in self.windows {
                        center_window(window.hwnd)
                            .unwrap_or_else(|e| log::error!("failed to center window: {e}"));
                    }
                });

            // RESIZE ALL combobox.
            ui.add_enabled_ui(self.resize_enabled, |ui| {
                egui::ComboBox::from_id_salt("resize-all")
                    .width(ui.available_width().min(120.0))
                    .selected_text("Resize All")
                    .show_ui(ui, |ui| {
                        resolution_ui(ui, Size2D::zero(), |size| {
                            for window in self.windows {
                                resize_window(window.hwnd, size)
                                    .unwrap_or_else(|e| log::error!("failed to resize window: {e}"));
                            }
                        });
                    });
            });

            // Checkbox — only shown for groups with a known executable path.
            self.resize_enabled_ui(ui);
        });

        ui.add_space(4.0);

        for window in self.windows {
            ui.push_id(window.hwnd.0, |ui| {
                ui.horizontal(|ui| window_header_ui(ui, window));
                ui.horizontal(|ui| window_command_ui(ui, window, self.resize_enabled));
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

fn window_command_ui(ui: &mut Ui, window: &WindowInfo, enabled: bool) {
    if window.is_centered == Some(true) {
        ui.add_sized((80.0, 16.0), Label::new(format!("{CHAR_CHECK}centered")));
    } else {
        ui.add_sized((80.0, 16.0), Button::new("CENTER"))
            .clicked()
            .then(|| {
                center_window(window.hwnd)
                    .unwrap_or_else(|e| log::error!("failed to center window: {e}"));
            });
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
                resolution_ui(ui, size, |size| {
                    resize_window(window.hwnd, size)
                        .unwrap_or_else(|e| log::error!("failed to resize window: {e}"));
                });
            });
    });
}

fn resolution_ui(
    ui: &mut Ui,
    selected: Size2D<u32>,
    action: impl Fn(Size2D<i32>)) {
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
                .then(|| action(resolution));
        }
        ui.label("");
    }
}

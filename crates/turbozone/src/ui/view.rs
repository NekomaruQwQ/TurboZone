use std::collections::BTreeMap;

use eframe::egui::*;
use turbozone_windows::{WindowHandle, WindowInfo, WindowState};
use turbozone_core::{
    is_known_resolution, ResolvedGroupId, WindowGroup, WindowSize, RESOLUTION_GROUPS,
};

use crate::app::Action;
use crate::configuration::ConfigState;

use super::color;
use super::widget::Card;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResizeSelection {
    Primary,
    Alternative(WindowSize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResizeControl {
    default_size: Option<WindowSize>,
}

impl ResizeControl {
    const fn new(default_size: Option<WindowSize>) -> Self {
        Self { default_size }
    }

    const fn target(self, selection: ResizeSelection) -> Option<WindowSize> {
        match selection {
            ResizeSelection::Primary => self.default_size,
            ResizeSelection::Alternative(size) => Some(size),
        }
    }
}

/// Renders the complete TurboRnR window and appends accepted native actions.
pub fn app_ui(
    ui: &mut Ui,
    groups: &BTreeMap<ResolvedGroupId, WindowGroup<WindowInfo>>,
    config: &ConfigState,
    native_error: Option<&str>,
    pending_actions: &mut Vec<Action>) {
    CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin::same(10)))
        .show(ui, |ui| {
            app_heading(ui, config);
            ScrollArea::vertical()
                .auto_shrink(false)
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    if let Some(ref error) = config.error {
                        error_card(ui, "Configuration", error);
                    }
                    if let Some(error) = native_error {
                        error_card(ui, "Windows", error);
                    }
                    if groups.is_empty() {
                        Card::default().show(ui, |ui| {
                            ui.label(RichText::new("No application windows found").weak());
                        });
                    }
                    for group in groups.values() {
                        group_card(ui, group, pending_actions);
                    }
                });
        });
}

fn app_heading(ui: &mut Ui, config: &ConfigState) {
    ui.heading("TurboRnR");
    let path = config.path.as_ref()
        .map_or_else(|| "Configuration path unavailable".to_owned(), |path| {
            path.display().to_string()
        });
    ui.add(Label::new(RichText::new(path).small().weak()).truncate());
    ui.add_space(8.0);
}

fn error_card(ui: &mut Ui, title: &str, error: &str) {
    Card::default().show(ui, |ui| {
        ui.label(RichText::new(title).strong().color(color::RED));
        ui.label(error);
    });
}

fn group_card(
    ui: &mut Ui,
    group: &WindowGroup<WindowInfo>,
    pending_actions: &mut Vec<Action>) {
    let (header_actions, body_actions) = Card::default().show_collapsible(
        ui,
        ("window-group", &group.id),
        |ui| group_header(ui, group),
        |ui| group_body(ui, group));
    pending_actions.extend(header_actions);
    pending_actions.extend(body_actions.unwrap_or_default());
}

fn group_header(ui: &mut Ui, group: &WindowGroup<WindowInfo>) -> Vec<Action> {
    let mut actions = Vec::new();
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        let handles = || group.windows.iter().map(|window| window.handle).collect::<Vec<_>>();
        if ui.button("CENTER ALL").clicked() {
            actions.push(Action::Center {
                windows: handles(),
            });
        }
        if group.allow_resize {
            group_resize_controls(ui, group, handles, &mut actions);
        } else {
            ui.label(RichText::new("RESIZE OFF").small().weak());
        }

        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(&group.name).heading());
            ui.label(RichText::new(format!("{} windows", group.windows.len())).small().weak());
        });
    });
    actions
}

fn group_resize_controls(
    ui: &mut Ui,
    group: &WindowGroup<WindowInfo>,
    handles: impl Fn() -> Vec<WindowHandle>,
    actions: &mut Vec<Action>) {
    let control = ResizeControl::new(group.default_size);
    let Some(default_size) = control.target(ResizeSelection::Primary) else {
        if let Some(size) = resize_menu_button(ui, "RESIZE") {
            actions.push(Action::Resize {
                windows: handles(),
                size,
            });
        }
        return;
    };

    if ui.button(format!("RESIZE {}x{}", default_size.width, default_size.height)).clicked() {
        actions.push(Action::Resize {
            windows: handles(),
            size: default_size,
        });
    }
    if let Some(size) = resize_menu_button(ui, "\u{25bc}")
        && let Some(target) = control.target(ResizeSelection::Alternative(size)) {
        actions.push(Action::Resize {
            windows: handles(),
            size: target,
        });
    }
}

fn group_body(ui: &mut Ui, group: &WindowGroup<WindowInfo>) -> Vec<Action> {
    let mut actions = Vec::new();
    if let Some(path) = group.executable.as_ref().and_then(|executable| executable.path.as_deref()) {
        ui.add(Label::new(RichText::new(path).small().weak()).truncate());
        ui.add_space(4.0);
    }
    for window in &group.windows {
        ui.push_id(window.handle.address(), |ui| {
            window_row(ui, window, group.allow_resize, group.default_size, &mut actions);
        });
    }
    actions
}

fn window_row(
    ui: &mut Ui,
    window: &WindowInfo,
    allow_resize: bool,
    default_size: Option<WindowSize>,
    actions: &mut Vec<Action>) {
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        window_controls(ui, window, allow_resize, default_size, actions);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let state = match window.state {
                WindowState::Normal => "",
                WindowState::Maximized => "[max]",
                WindowState::Minimized => "[min]",
            };
            if !state.is_empty() {
                ui.label(RichText::new(state).small().weak());
            }
            ui.add(Label::new(&window.window_title).truncate());
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("PID {}", window.process_id)).small().weak());
        if let Some(ref name) = window.executable_name {
            ui.label(RichText::new(name).small().weak());
        }
        if let Some(size) = window.client_size {
            let text = RichText::new(format!("{}x{}", size.width, size.height)).small();
            ui.label(if is_known_resolution(size) {
                text.color(color::GREEN)
            } else {
                text.weak()
            });
        }
    });
    ui.add_space(4.0);
}

fn window_controls(
    ui: &mut Ui,
    window: &WindowInfo,
    allow_resize: bool,
    default_size: Option<WindowSize>,
    actions: &mut Vec<Action>) {
    match window.is_centered {
        Some(true) => {
            ui.label(RichText::new("CENTERED").small().color(color::GREEN));
        },
        Some(false) | None => {
            if ui.button("CENTER").clicked() {
                actions.push(Action::Center {
                    windows: vec![window.handle],
                });
            }
        },
    }

    if !allow_resize {
        return;
    }
    let control = ResizeControl::new(default_size);
    let Some(default_size) = control.target(ResizeSelection::Primary) else {
        if let Some(size) = resize_menu_button(ui, "RESIZE") {
            actions.push(Action::Resize {
                windows: vec![window.handle],
                size,
            });
        }
        return;
    };

    if ui.button("RESIZE")
        .on_hover_text(format!(
            "Resize to {}x{}",
            default_size.width,
            default_size.height))
        .clicked() {
        actions.push(Action::Resize {
            windows: vec![window.handle],
            size: default_size,
        });
    }
    if let Some(size) = resize_menu_button(ui, "\u{25bc}")
        && let Some(target) = control.target(ResizeSelection::Alternative(size)) {
        actions.push(Action::Resize {
            windows: vec![window.handle],
            size: target,
        });
    }
}

fn resize_menu_button(ui: &mut Ui, label: &str) -> Option<WindowSize> {
    ui.menu_button(label, resize_menu).inner.flatten()
}

fn resize_menu(ui: &mut Ui) -> Option<WindowSize> {
    let mut selected = None;
    for &(name, resolutions) in RESOLUTION_GROUPS {
        ui.label(RichText::new(name).small().weak());
        for &size in resolutions {
            if ui.button(format!("{} x {}", size.width, size.height)).clicked() {
                selected = Some(size);
                ui.close();
            }
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternative_resize_does_not_replace_configured_default() {
        let default = WindowSize::new(1440, 900);
        let alternative = WindowSize::new(1920, 1200);
        let control = ResizeControl::new(Some(default));

        assert_eq!(
            (
                control.target(ResizeSelection::Alternative(alternative)),
                control.target(ResizeSelection::Primary),
            ),
            (Some(alternative), Some(default)));
    }
}

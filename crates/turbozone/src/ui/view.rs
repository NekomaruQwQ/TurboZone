use eframe::egui::*;
use turbozone_core::{
    is_known_window_size, RuntimeMove, RuntimeResize, RuntimeRule, Size2D,
    WINDOW_SIZE_MANIFEST,
};
use turbozone_windows::{WindowHandle, WindowInfo, WindowState};

use crate::app::Action;
use crate::configuration::ConfigState;
use crate::data::{SectionedWindows, WindowPage, WindowSection};

use super::color;
use super::widget::Card;

/// Renders the complete TurboZone window and appends accepted native actions.
pub fn app_ui(
    ui: &mut Ui,
    windows: &SectionedWindows,
    config: &ConfigState,
    native_error: Option<&str>,
    page: &mut WindowPage,
    pending_actions: &mut Vec<Action>) {
    CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin::same(10)))
        .show(ui, |ui| {
            app_heading(ui, config, windows.diagnostic_count(), page);
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
                    match *page {
                        WindowPage::Sections => {
                            sections_page(ui, windows, config, pending_actions);
                        },
                        WindowPage::Unmatched => diagnostics_page(ui, windows),
                    }
                });
        });
}

fn app_heading(
    ui: &mut Ui,
    config: &ConfigState,
    diagnostic_count: usize,
    page: &mut WindowPage) {
    ui.horizontal(|ui| {
        ui.heading("TurboZone");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.selectable_value(
                page,
                WindowPage::Unmatched,
                format!("UNMATCHED ({diagnostic_count})"));
            ui.selectable_value(page, WindowPage::Sections, "SECTIONS");
        });
    });
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

fn sections_page(
    ui: &mut Ui,
    windows: &SectionedWindows,
    config: &ConfigState,
    pending_actions: &mut Vec<Action>) {
    if windows.sections.is_empty() {
        Card::default().show(ui, |ui| {
            ui.label(RichText::new("No matched windows found").weak());
        });
        return;
    }
    for section in &windows.sections {
        let Some(rule) = config.runtime.rules.get(section.rule_index) else {
            continue;
        };
        section_card(ui, section, rule, pending_actions);
    }
}

fn section_card(
    ui: &mut Ui,
    section: &WindowSection,
    rule: &RuntimeRule,
    pending_actions: &mut Vec<Action>) {
    let (header_actions, body_actions) = Card::default().show_collapsible(
        ui,
        ("window-section", rule.name.as_str(), section.executable_path.as_str()),
        |ui| section_header(ui, section, rule),
        |ui| section_body(ui, section, rule));
    pending_actions.extend(header_actions);
    pending_actions.extend(body_actions.unwrap_or_default());
}

fn section_header(ui: &mut Ui, section: &WindowSection, rule: &RuntimeRule) -> Vec<Action> {
    let mut actions = Vec::new();
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        let handles = || {
            section.windows.iter()
                .map(|window| window.handle)
                .collect::<Vec<_>>()
        };
        if rule.r#move == RuntimeMove::Center && ui.button("CENTER ALL").clicked() {
            actions.push(Action::Center {
                windows: handles(),
            });
        }
        if rule.resize.enabled {
            section_resize_controls(ui, rule.resize, handles, &mut actions);
        }
        if rule.r#move == RuntimeMove::Disabled && !rule.resize.enabled {
            ui.label(RichText::new("READ ONLY").small().weak());
        }

        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(
                rule.description.as_deref().unwrap_or(&rule.name)).heading());
            ui.label(RichText::new(format!("{} windows", section.windows.len())).small().weak());
        });
    });
    actions
}

fn section_resize_controls(
    ui: &mut Ui,
    resize: RuntimeResize,
    handles: impl Fn() -> Vec<WindowHandle>,
    actions: &mut Vec<Action>) {
    let Some(primary_size) = resize.primary_size() else {
        if let Some(size) = resize_menu_button(ui, "RESIZE", resize) {
            actions.push(Action::Resize {
                windows: handles(),
                size,
            });
        }
        return;
    };

    if ui.button(format!("RESIZE {}x{}", primary_size.width, primary_size.height)).clicked() {
        actions.push(Action::Resize {
            windows: handles(),
            size: primary_size,
        });
    }
    if let Some(size) = resize_menu_button(ui, "\u{25bc}", resize) {
        actions.push(Action::Resize {
            windows: handles(),
            size,
        });
    }
}

fn section_body(ui: &mut Ui, section: &WindowSection, rule: &RuntimeRule) -> Vec<Action> {
    let mut actions = Vec::new();
    if let Some(path) = section.windows.first()
        .and_then(|window| window.executable_path.as_deref()) {
        ui.add(Label::new(RichText::new(path).small().weak()).truncate());
        ui.add_space(4.0);
    }
    for window in &section.windows {
        ui.push_id(window.handle.address(), |ui| {
            window_row(ui, window, rule, &mut actions);
        });
    }
    actions
}

fn window_row(
    ui: &mut Ui,
    window: &WindowInfo,
    rule: &RuntimeRule,
    actions: &mut Vec<Action>) {
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        window_controls(ui, window, rule, actions);
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
    window_metadata(ui, window, false);
    ui.add_space(4.0);
}

fn window_controls(
    ui: &mut Ui,
    window: &WindowInfo,
    rule: &RuntimeRule,
    actions: &mut Vec<Action>) {
    if rule.r#move == RuntimeMove::Center {
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
    }

    if !rule.resize.enabled {
        return;
    }
    let Some(primary_size) = rule.resize.primary_size() else {
        if let Some(size) = resize_menu_button(ui, "RESIZE", rule.resize) {
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
            primary_size.width,
            primary_size.height))
        .clicked() {
        actions.push(Action::Resize {
            windows: vec![window.handle],
            size: primary_size,
        });
    }
    if let Some(size) = resize_menu_button(ui, "\u{25bc}", rule.resize) {
        actions.push(Action::Resize {
            windows: vec![window.handle],
            size,
        });
    }
}

fn resize_menu_button(
    ui: &mut Ui,
    label: &str,
    resize: RuntimeResize) -> Option<Size2D<i32>> {
    ui.menu_button(label, |ui| resize_menu(ui, resize)).inner.flatten()
}

fn resize_menu(ui: &mut Ui, resize: RuntimeResize) -> Option<Size2D<i32>> {
    let mut selected = None;
    let mut available = false;
    for &(name, resolutions) in WINDOW_SIZE_MANIFEST {
        let resolutions = resolutions.iter()
            .copied()
            .filter(|&size| resize.allows_selector_size(size));
        let mut heading_shown = false;
        for size in resolutions {
            available = true;
            if !heading_shown {
                ui.label(RichText::new(name).small().weak());
                heading_shown = true;
            }
            if ui.button(format!("{} x {}", size.width, size.height)).clicked() {
                selected = Some(size);
                ui.close();
            }
        }
    }
    if !available {
        ui.label(RichText::new("No sizes within configured limits").weak());
    }
    selected
}

fn diagnostics_page(ui: &mut Ui, windows: &SectionedWindows) {
    if windows.diagnostic_count() == 0 {
        Card::default().show(ui, |ui| {
            ui.label(RichText::new("No unmatched windows found").weak());
        });
        return;
    }
    diagnostic_list(
        ui,
        "Unmatched windows",
        "These windows have executable paths but match no rule.",
        &windows.unmatched_windows);
    diagnostic_list(
        ui,
        "Executable path unavailable",
        "These windows are intentionally excluded from rule matching.",
        &windows.unknown_windows);
}

fn diagnostic_list(ui: &mut Ui, title: &str, explanation: &str, windows: &[WindowInfo]) {
    if windows.is_empty() {
        return;
    }
    Card::default().show(ui, |ui| {
        ui.label(RichText::new(title).heading());
        ui.label(RichText::new(explanation).small().weak());
        ui.add_space(6.0);
        for window in windows {
            ui.push_id(("diagnostic-window", window.handle.address()), |ui| {
                ui.add(Label::new(&window.window_title).truncate());
                window_metadata(ui, window, true);
                ui.add_space(4.0);
            });
        }
    });
}

fn window_metadata(ui: &mut Ui, window: &WindowInfo, show_path: bool) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("PID {}", window.process_id)).small().weak());
        if let Some(ref name) = window.executable_name {
            ui.label(RichText::new(name).small().weak());
        }
        if let Some(size) = window.client_size {
            let text = RichText::new(format!("{}x{}", size.width, size.height)).small();
            ui.label(if is_known_window_size(size) {
                text.color(color::GREEN)
            } else {
                text.weak()
            });
        }
    });
    if show_path {
        let path = window.executable_path.as_deref().unwrap_or("Executable path unavailable");
        ui.add(Label::new(RichText::new(path).small().weak()).truncate());
    }
}

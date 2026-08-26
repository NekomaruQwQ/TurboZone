use eframe::egui::*;
use euclid::default::Size2D;
use turbozone_core::{
    is_known_window_size, ResizeLimits, RuntimeRule, WindowInfo, WindowState, WINDOW_SIZE_MANIFEST,
};
use turbozone_windows::WindowHandle;

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
                        WindowPage::Diagnostics => diagnostics_page(ui, windows),
                    }
                });
        });
}

/// Renders page navigation and the active configuration path.
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
                WindowPage::Diagnostics,
                format!("DIAGNOSTICS ({diagnostic_count})"));
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

/// Shows a top-level failure without hiding the remaining application state.
fn error_card(ui: &mut Ui, title: &str, error: &str) {
    Card::default().show(ui, |ui| {
        ui.label(RichText::new(title).strong().color(color::RED));
        ui.label(error);
    });
}

/// Renders successfully classified windows in configuration order.
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

/// Keeps collapse state keyed by the stable rule and program identity.
fn section_card(
    ui: &mut Ui,
    section: &WindowSection,
    rule: &RuntimeRule,
    pending_actions: &mut Vec<Action>) {
    let (header_actions, body_actions) = Card::default().show_collapsible(
        ui,
        ("window-section", rule.name.as_str(), section.program_path.as_str()),
        |ui| section_header(ui, section, rule),
        |ui| section_body(ui, section, rule));
    pending_actions.extend(header_actions);
    pending_actions.extend(body_actions.unwrap_or_default());
}

/// Offers section actions only for handles with complete snapshot details.
fn section_header(ui: &mut Ui, section: &WindowSection, rule: &RuntimeRule) -> Vec<Action> {
    let mut actions = Vec::new();
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        let handles = || {
            section.windows.iter()
                .filter(|window| window.detail.is_ok())
                .map(|window| window.handle)
                .collect::<Vec<_>>()
        };
        if rule.relocate && ui.button("CENTER ALL").clicked() {
            actions.push(Action::Center {
                windows: handles(),
            });
        }
        section_resize_controls(ui, rule, handles, &mut actions);
        if !rule.relocate && rule.resize_exact.is_none() && rule.resize_limits.is_none() {
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

/// Renders exact-only, selector-only, or primary-plus-selector resize controls.
fn section_resize_controls(
    ui: &mut Ui,
    rule: &RuntimeRule,
    handles: impl Fn() -> Vec<WindowHandle>,
    actions: &mut Vec<Action>) {
    let primary_size = rule.resize_exact
        .or_else(|| rule.resize_limits.as_ref()?.default);
    let Some(primary_size) = primary_size else {
        if let Some(size) = rule.resize_limits.as_ref()
            .and_then(|limits| resize_menu_button(ui, "RESIZE", limits)) {
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
    if let Some(size) = rule.resize_limits.as_ref()
        .and_then(|limits| resize_menu_button(ui, "\u{25bc}", limits)) {
        actions.push(Action::Resize {
            windows: handles(),
            size,
        });
    }
}

/// Shows the program path and independently actionable window rows.
fn section_body(ui: &mut Ui, section: &WindowSection, rule: &RuntimeRule) -> Vec<Action> {
    let mut actions = Vec::new();
    if let Some(detail) = section.windows.first()
        .and_then(|window| window.detail.as_ref().ok()) {
        ui.add(Label::new(RichText::new(&detail.program_path).small().weak()).truncate());
        ui.add_space(4.0);
    }
    for window in &section.windows {
        ui.push_id(window.handle.address(), |ui| {
            window_row(ui, window, rule, &mut actions);
        });
    }
    actions
}

/// Renders a window title, visual state, controls, and detail status.
fn window_row(
    ui: &mut Ui,
    window: &WindowInfo<WindowHandle>,
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
            ui.add(Label::new(&window.title).truncate());
        });
    });
    window_metadata(ui, window, false);
    ui.add_space(4.0);
}

/// Appends actions only when details are complete and the rule enables them.
fn window_controls(
    ui: &mut Ui,
    window: &WindowInfo<WindowHandle>,
    rule: &RuntimeRule,
    actions: &mut Vec<Action>) {
    // Never expose actions for an incomplete snapshot, even if passed a matching rule.
    let Ok(ref detail) = window.detail else {
        return;
    };
    if rule.relocate {
        if detail.is_centered() {
            ui.label(RichText::new("CENTERED").small().color(color::GREEN));
        } else {
            let response = ui.button("CENTER");
            if response.clicked() {
                actions.push(Action::Center {
                    windows: vec![window.handle],
                });
            }
        }
    }

    let primary_size = rule.resize_exact
        .or_else(|| rule.resize_limits.as_ref()?.default);
    let Some(primary_size) = primary_size else {
        if let Some(size) = rule.resize_limits.as_ref()
            .and_then(|limits| resize_menu_button(ui, "RESIZE", limits)) {
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
    if let Some(size) = rule.resize_limits.as_ref()
        .and_then(|limits| resize_menu_button(ui, "\u{25bc}", limits)) {
        actions.push(Action::Resize {
            windows: vec![window.handle],
            size,
        });
    }
}

/// Returns a selected size without performing native work during rendering.
fn resize_menu_button(
    ui: &mut Ui,
    label: &str,
    resize: &ResizeLimits) -> Option<Size2D<i32>> {
    ui.menu_button(label, |ui| resize_menu(ui, resize)).inner.flatten()
}

/// Filters manifest choices through inclusive selector bounds.
fn resize_menu(ui: &mut Ui, resize: &ResizeLimits) -> Option<Size2D<i32>> {
    let mut selected = None;
    let mut available = false;
    for &(name, resolutions) in WINDOW_SIZE_MANIFEST {
        let resolutions = resolutions.iter()
            .copied()
            .filter(|&size| resize.allows_size(size));
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

/// Separates complete unmatched windows from failed-detail snapshots.
fn diagnostics_page(ui: &mut Ui, windows: &SectionedWindows) {
    if windows.diagnostic_count() == 0 {
        Card::default().show(ui, |ui| {
            ui.label(RichText::new("No window diagnostics").weak());
        });
        return;
    }
    diagnostic_list(
        ui,
        "Unmatched windows",
        "These windows have complete details but match no rule.",
        &windows.unmatched_windows);
    diagnostic_list(
        ui,
        "Details unavailable",
        "These windows could not be fully queried. Matching and controls are unavailable until a refresh succeeds.",
        &windows.failed_windows);
}

/// Shows retained identity and diagnostics without exposing native actions.
fn diagnostic_list(ui: &mut Ui, title: &str, explanation: &str, windows: &[WindowInfo<WindowHandle>]) {
    if windows.is_empty() {
        return;
    }
    Card::default().show(ui, |ui| {
        ui.label(RichText::new(title).heading());
        ui.label(RichText::new(explanation).small().weak());
        ui.add_space(6.0);
        for window in windows {
            ui.push_id(("diagnostic-window", window.handle.address()), |ui| {
                ui.add(Label::new(&window.title).truncate());
                ui.label(RichText::new(format!(
                    "{:?} | HWND 0x{:x}", window.state, window.handle.address())).small().weak());
                window_metadata(ui, window, true);
                ui.add_space(4.0);
            });
        }
    });
}

/// Shows complete metadata or contextual failure messages, never fabricated values.
fn window_metadata(ui: &mut Ui, window: &WindowInfo<WindowHandle>, show_path: bool) {
    let detail = match window.detail {
        Ok(ref detail) => detail,
        Err(ref errors) => {
            ui.label(RichText::new("DETAILS UNAVAILABLE").small().color(color::RED));
            for error in errors {
                ui.label(RichText::new(error).small());
            }
            return;
        },
    };
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("PID {}", detail.process_id)).small().weak());
        ui.label(RichText::new(&detail.program_name).small().weak());
        let size = detail.content_rect.size;
        let text = RichText::new(format!("{}x{}", size.width, size.height)).small();
        ui.label(if is_known_window_size(size) {
            text.color(color::GREEN)
        } else {
            text.weak()
        });
    });
    if show_path {
        ui.add(Label::new(RichText::new(&detail.program_path).small().weak()).truncate());
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::{Point2D, Rect as PixelRect};
    use turbozone_core::{Config, ConfigRule, ResizeRule, WindowDetail};

    use super::*;

    /// Captures text emitted by a headless UI pass; no native windows or actions are needed.
    fn rendered_text(mut render: impl FnMut(&mut Ui)) -> Vec<String> {
        let mut output = Context::default().run_ui(RawInput::default(), |ui| render(ui));
        // Headless tests inspect shapes without a renderer to consume texture uploads.
        output.textures_delta.clear();
        output.shapes.into_iter().filter_map(|shape| match shape.shape {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        }).collect()
    }

    /// Builds a validated action rule for a single rendering test.
    fn rule(resize: ResizeRule) -> RuntimeRule {
        Config { rules: vec![ConfigRule {
            name: "app".to_owned(),
            relocate: true,
            resize,
            ..Default::default()
        }] }.validate().unwrap().rules.remove(0)
    }

    /// Constructs a complete snapshot whose handle is never sent to the OS.
    fn window() -> WindowInfo<WindowHandle> {
        WindowInfo {
            handle: WindowHandle::default(),
            title: "App".to_owned(),
            state: WindowState::Normal,
            detail: Ok(WindowDetail {
                monitor_rect: PixelRect::new(Point2D::zero(), Size2D::new(1920, 1080)),
                content_rect: PixelRect::new(Point2D::zero(), Size2D::new(640, 480)),
                process_id: 42,
                program_path: "C:/Apps/App.exe".to_owned(),
                program_name: "App.exe".to_owned(),
            }),
        }
    }

    #[test]
    fn failed_details_render_errors_without_fabricated_metadata() {
        let mut window = window();
        window.detail = Err(vec!["Monitor query failed".to_owned(), "Program access denied".to_owned()]);
        let text = rendered_text(|ui| window_metadata(ui, &window, true));
        assert_eq!(text, ["DETAILS UNAVAILABLE", "Monitor query failed", "Program access denied"]);
    }

    #[test]
    fn complete_details_render_program_and_size() {
        let text = rendered_text(|ui| window_metadata(ui, &window(), true));
        assert_eq!(text, ["PID 42", "App.exe", "640x480", "C:/Apps/App.exe"]);
    }

    #[test]
    fn failed_details_offer_no_native_controls() {
        let mut window = window();
        window.detail = Err(vec!["Client query failed".to_owned()]);
        let rule = rule(ResizeRule::Boolean(true));
        let mut actions = Vec::new();
        let text = rendered_text(|ui| window_controls(ui, &window, &rule, &mut actions));
        assert!(text.is_empty() && actions.is_empty());
    }

    #[test]
    fn exact_resize_offers_only_a_primary_button() {
        let rule = rule(ResizeRule::Exact { exact: Size2D::new(1280, 720) });
        let text = rendered_text(|ui| {
            section_resize_controls(ui, &rule, Vec::new, &mut Vec::new());
        });
        assert_eq!(text, ["RESIZE 1280x720"]);
    }

    #[test]
    fn selector_default_offers_a_primary_button_and_menu() {
        let rule = rule(ResizeRule::Selector(ResizeLimits {
            default: Some(Size2D::new(1280, 720)),
            ..Default::default()
        }));
        let text = rendered_text(|ui| {
            section_resize_controls(ui, &rule, Vec::new, &mut Vec::new());
        });
        assert_eq!(text, ["RESIZE 1280x720", "\u{25bc}"]);
    }

    #[test]
    fn disabled_resize_offers_no_controls() {
        let rule = rule(ResizeRule::Boolean(false));
        let text = rendered_text(|ui| {
            section_resize_controls(ui, &rule, Vec::new, &mut Vec::new());
        });
        assert_eq!(text, Vec::<String>::new());
    }

    #[test]
    fn unbounded_resize_offers_only_a_selector() {
        let rule = rule(ResizeRule::Boolean(true));
        let text = rendered_text(|ui| {
            section_resize_controls(ui, &rule, Vec::new, &mut Vec::new());
        });
        assert_eq!(text, ["RESIZE"]);
    }
}

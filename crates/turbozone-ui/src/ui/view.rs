//! Generic window-section rendering that emits core actions without native work.

use std::fmt::Debug;
use std::hash::Hash;

use eframe::egui::*;
use euclid::default::Size2D;
use turbozone_core::{
    WindowAction, ResizeSelector, RuntimeConfig, RuntimeRule, WindowInfo,
    WindowSection, WindowState,
    constants::STANDARD_SIZE,
};

use super::color;
use super::widget::Card;

/// Renders matched windows and returns accepted actions for deferred execution.
///
/// Rendering remains a pure snapshot operation: no backend is borrowed and no native
/// work can occur within egui's UI pass.
pub fn app_ui<H>(
    ui: &mut Ui,
    windows: &[WindowSection<H>],
    config: &RuntimeConfig) -> Vec<WindowAction<H>>
where
    H: Copy + Debug + Eq + Hash + 'static,
{
    let mut actions = Vec::new();
    CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin::same(10)))
        .show(ui, |ui| {
            ui.heading("TurboZone");
            ui.add_space(8.0);
            ScrollArea::vertical()
                .auto_shrink(false)
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| sections_page(ui, windows, config, &mut actions));
        });
    actions
}

/// Resolves every section through its stable name so config reordering cannot retarget it.
fn sections_page<H>(
    ui: &mut Ui,
    windows: &[WindowSection<H>],
    config: &RuntimeConfig,
    actions: &mut Vec<WindowAction<H>>)
where
    H: Copy + Debug + Eq + Hash + 'static,
{
    if windows.is_empty() {
        Card::default().show(ui, |ui| {
            ui.label(RichText::new("No matched windows found").weak());
        });
        return;
    }
    for section in windows {
        let Some(rule) = config.rule(&section.rule_name) else { continue; };
        section_card(ui, section, rule, actions);
    }
}

/// Keeps collapse state keyed by stable rule and program identities.
fn section_card<H>(
    ui: &mut Ui,
    section: &WindowSection<H>,
    rule: &RuntimeRule,
    actions: &mut Vec<WindowAction<H>>)
where
    H: Copy + Debug + Eq + Hash + 'static,
{
    let (header_actions, body_actions) = Card::default().show_collapsible(
        ui,
        ("window-section", rule.name.as_str(), section.program_path.as_str()),
        |ui| section_header(ui, section, rule),
        |ui| section_body(ui, section, rule));
    actions.extend(header_actions);
    actions.extend(body_actions.unwrap_or_default());
}

/// Offers section actions only for handles with complete snapshot details.
fn section_header<H>(
    ui: &mut Ui,
    section: &WindowSection<H>,
    rule: &RuntimeRule) -> Vec<WindowAction<H>>
where
    H: Copy,
{
    let mut actions = Vec::new();
    let available = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(available, Layout::right_to_left(Align::Center), |ui| {
        if rule.relocate && ui.button("CENTER ALL").clicked() {
            actions.extend(actionable_handles(section).map(WindowAction::MoveToCenter));
        }
        section_resize_controls(ui, rule, section, &mut actions);
        if !rule.relocate && rule.resize_exact.is_none() && rule.resize_selector.is_none() {
            ui.label(RichText::new("READ ONLY").small().weak());
        }

        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(
                rule.description.as_deref().unwrap_or(rule.name.as_str())).heading());
            ui.label(RichText::new(format!("{} windows", section.windows.len())).small().weak());
        });
    });
    actions
}

/// Renders exact-only, selector-only, or primary-plus-selector resize controls.
fn section_resize_controls<H: Copy>(
    ui: &mut Ui,
    rule: &RuntimeRule,
    section: &WindowSection<H>,
    actions: &mut Vec<WindowAction<H>>) {
    let primary_size = rule.resize_exact
        .or_else(|| rule.resize_selector.as_ref()?.default.map(Size2D::from));
    let Some(primary_size) = primary_size else {
        if let Some(size) = rule.resize_selector.as_ref()
            .and_then(|selector| resize_menu_button(ui, "RESIZE", selector)) {
            actions.extend(actionable_handles(section).map(|handle| WindowAction::Resize(handle, size)));
        }
        return;
    };

    if ui.button(format!("RESIZE {}x{}", primary_size.width, primary_size.height)).clicked() {
        actions.extend(actionable_handles(section)
            .map(|handle| WindowAction::Resize(handle, primary_size)));
    }
    if let Some(size) = rule.resize_selector.as_ref()
        .and_then(|selector| resize_menu_button(ui, "\u{25bc}", selector)) {
        actions.extend(actionable_handles(section).map(|handle| WindowAction::Resize(handle, size)));
    }
}

/// Iterates only handles whose native metadata was complete when rendered.
fn actionable_handles<H: Copy>(section: &WindowSection<H>) -> impl Iterator<Item = H> + '_ {
    section.windows.iter()
        .filter(|window| window.detail.is_ok())
        .map(|window| window.handle)
}

/// Shows the program path and independently actionable window rows.
fn section_body<H>(
    ui: &mut Ui,
    section: &WindowSection<H>,
    rule: &RuntimeRule) -> Vec<WindowAction<H>>
where
    H: Copy + Debug + Eq + Hash + 'static,
{
    let mut actions = Vec::new();
    if let Some(detail) = section.windows.first()
        .and_then(|window| window.detail.as_ref().ok()) {
        ui.add(Label::new(RichText::new(detail.program.path.as_str()).small().weak()).truncate());
        ui.add_space(4.0);
    }
    for window in &section.windows {
        ui.push_id(window.handle, |ui| {
            window_row(ui, window, rule, &mut actions);
        });
    }
    actions
}

/// Renders a window title, visual state, controls, and detail status.
fn window_row<H: Copy>(
    ui: &mut Ui,
    window: &WindowInfo<H>,
    rule: &RuntimeRule,
    actions: &mut Vec<WindowAction<H>>) {
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
            ui.add(Label::new(window.title.as_str()).truncate());
        });
    });
    window_metadata(ui, window);
    ui.add_space(4.0);
}

/// Appends actions only when details are complete and the rule enables them.
fn window_controls<H: Copy>(
    ui: &mut Ui,
    window: &WindowInfo<H>,
    rule: &RuntimeRule,
    actions: &mut Vec<WindowAction<H>>) {
    // Incomplete snapshots remain visible to diagnostics but never become actionable.
    let Ok(ref detail) = window.detail else { return; };
    if rule.relocate {
        if detail.monitor_rect.center() != detail.content_rect.center() {
            if ui.button("CENTER").clicked() {
                actions.push(WindowAction::MoveToCenter(window.handle));
            }
        } else {
            ui.label(RichText::new("CENTERED").small().color(color::GREEN));
        }
    }

    let primary_size = rule.resize_exact
        .or_else(|| rule.resize_selector.as_ref()?.default.map(Size2D::from));
    let Some(primary_size) = primary_size else {
        if let Some(size) = rule.resize_selector.as_ref()
            .and_then(|selector| resize_menu_button(ui, "RESIZE", selector)) {
            actions.push(WindowAction::Resize(window.handle, size));
        }
        return;
    };
    if ui.button("RESIZE")
        .on_hover_text(format!(
            "Resize to {}x{}",
            primary_size.width,
            primary_size.height))
        .clicked() {
        actions.push(WindowAction::Resize(window.handle, primary_size));
    }
    if let Some(size) = rule.resize_selector.as_ref()
        .and_then(|selector| resize_menu_button(ui, "\u{25bc}", selector)) {
        actions.push(WindowAction::Resize(window.handle, size));
    }
}

/// Returns a selected size without performing native work during rendering.
fn resize_menu_button(
    ui: &mut Ui,
    label: &str,
    resize: &ResizeSelector) -> Option<Size2D<i32>> {
    ui.menu_button(label, |ui| resize_menu(ui, resize)).inner.flatten()
}

/// Filters manifest choices through inclusive selector bounds.
fn resize_menu(ui: &mut Ui, resize: &ResizeSelector) -> Option<Size2D<i32>> {
    let mut selected = None;
    let mut available = false;
    for &(name, resolutions) in STANDARD_SIZE {
        let resolutions = resolutions.iter()
            .copied()
            .map(Size2D::from)
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

/// Shows metadata only for complete snapshots; failed queries are logged by core.
fn window_metadata<H>(ui: &mut Ui, window: &WindowInfo<H>) {
    let Ok(ref detail) = window.detail else { return; };
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("PID {}", detail.process_id)).small().weak());
        ui.label(RichText::new(detail.program.description.as_str()).small().weak());
        let size = detail.content_rect.size;
        let text = RichText::new(format!("{}x{}", size.width, size.height)).small();
        let known = STANDARD_SIZE.iter()
            .any(|&(_, sizes)| sizes.contains(&[size.width, size.height]));
        ui.label(if known { text.color(color::GREEN) } else { text.weak() });
    });
}

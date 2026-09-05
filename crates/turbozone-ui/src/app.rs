use turbozone_core::*;

use std::time::Duration;
use std::time::Instant;

use tap::prelude::*;

use euclid::default::Size2D;

use eframe::egui;
use egui::*;
use egui_phosphor_icons::icons;

/// Standard client-area sizes, ordered from largest to smallest.
pub const STANDARD_SIZE: &[(&str, &[[i32; 2]])] = &[
    ("16:10", &[
        [3840, 2400],
        [2880, 1800],
        [2560, 1600],
        [1920, 1200],
        [1680, 1050],
        [1440, 900],
        [1280, 800],
        [960, 600],
    ]),
    ("16:9", &[
        [3840, 2160],
        [2880, 1620],
        [2560, 1440],
        [1920, 1080],
        [1600, 900],
        [1360, 768],
        [1280, 720],
        [1024, 576],
        [960, 540],
    ]),
];

/// Fixed viewport chosen to keep the M0 layout comfortable with current metadata.
pub const TURBOZONE_WINDOW_SIZE: [f32; 2] = [450.0, 720.0];

/// Native state changes less often than the UI paints, so snapshots use a separate cadence.
const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

/// Painting stays responsive without coupling native enumeration to every frame.
const RENDER_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);

const CHAR_EMPTY: char = '\u{26ab}';
const CHAR_CHECK: char = '\u{2705}';
const CHAR_CROSS: char = '\u{00d7}';
const CHAR_WINDOW: char = '\u{1f5d6}';

/// TurboZone application state shared by the logic and UI phases.
///
/// Core's engine owns native state and queued effects. This layer owns framework timing
/// and turns one immutable snapshot into an ordered batch of user-requested actions.
pub struct App<B: Backend> {
    engine: Engine<B>,
    last_update: Instant,
}

impl<B: Backend> eframe::App for App<B> {
    fn logic(&mut self, _: &Context, _: &mut eframe::Frame) {
        if  self.engine.has_pending_actions() ||
            self.last_update.elapsed() >= UPDATE_INTERVAL {
            self.engine.tick();
            self.last_update = Instant::now();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let mut actions = Vec::new();

        CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    Self::app_ui(
                        ui,
                        &mut self.engine,
                        &mut |action| actions.push(action));
                });
        });

        let request_repaint = !actions.is_empty();
        self.engine.queue(actions);

        if request_repaint {
            ui.request_repaint();
        } else {
            ui.request_repaint_after(RENDER_INTERVAL);
        }
    }
}

impl<B: Backend> App<B> {
    /// Takes ownership of verified authored rules and captures the initial snapshot.
    pub fn new(rules: Vec<Rule>, backend: B) -> Self {
        let mut engine = Engine::new(rules, backend);
        engine.tick();

        Self {
            engine,
            last_update: Instant::now(),
        }
    }

    pub fn viewport() -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_inner_size(TURBOZONE_WINDOW_SIZE)
            .with_min_inner_size(TURBOZONE_WINDOW_SIZE)
            .with_max_inner_size(TURBOZONE_WINDOW_SIZE)
            .with_resizable(false)
            .with_maximize_button(false)
    }

    pub fn setup_icon_font(fonts: &mut egui::FontDefinitions) {
        egui_phosphor_icons::add_fonts(fonts);
    }

    /// Renders a snapshot without native effects and emits actions through the callback.
    ///
    /// Group identity combines the stable rule name with its representative display path;
    /// window identity uses the backend handle. This keeps egui state stable while leaving
    /// all native mutation behind the engine queue boundary.
    pub fn app_ui<F: FnMut(WindowAction<B::Handle>)>(
        ui: &mut Ui,
        engine: &mut Engine<B>,
        action_callback: &mut F) {
        let groups = engine.groups();

        if !groups.is_empty() {
            for group in groups {
                let group_id = (
                    group.rule_name.as_str(),
                    group.program.path.as_str());
                let rule =
                    engine
                        .rule(&group.rule_name)
                        .expect("a rendered group must reference an active rule");
                ui.push_id(group_id, |ui| {
                    Self::group_ui(ui, group, rule, action_callback);
                });
            }
        } else {
            ui.weak("- nothing here -");
        }
    }

    /// Uses the stable rule name for headings and queries resize availability on demand.
    fn group_ui<F: FnMut(WindowAction<B::Handle>)>(
        ui: &mut Ui,
        group: &Group<B::Handle>,
        rule: &Rule,
        action_callback: &mut F) {
        ui.horizontal(|ui| {
            ui.heading(group.program.description.as_str());
            ui.add_space(4.0);
            ui.monospace(rule.name.as_str());

            // ui.weak(format!(
            //     "({}, {:+})",
            //     rule.display_name(),
            //     rule.priority));
        });

        ui.add(Label::new(RichText::new(group.program.path.as_str()).weak()).truncate());
        ui.add_space(4.0);

        if  rule.relocate ||
            rule.resize.selector().is_some() ||
            rule.resize.primary_size().is_some() {
            ui.horizontal(|ui| Self::group_action_ui(ui, group, rule, action_callback));
            ui.add_space(4.0);
        }

        for window in &group.windows {
            ui.push_id(window.handle, |ui| {
                ui.style_mut().spacing.item_spacing.x = 4.0;
                ui.horizontal(|ui| Self::window_header_ui(ui, window));
                ui.horizontal(|ui| Self::window_action_ui(ui, window, rule, action_callback));
            });
            ui.add_space(2.0);
        }
        ui.add_space(8.0);
    }

    /// Applies authored primary targets independently of selector limits to every group member.
    fn group_action_ui<F: FnMut(WindowAction<B::Handle>)>(
        ui: &mut Ui,
        group: &Group<B::Handle>,
        rule: &Rule,
        action_callback: &mut F) {
        if rule.relocate && ui.add_sized((80.0, 16.0), Button::new("CENTER")).clicked() {
            for window in &group.windows {
                action_callback(WindowAction::MoveToCenter(window.handle));
            }
        }

        if let Some(size) = rule.resize.primary_size() {
            let response = ui.button(format!("RESIZE TO {}x{}", size.width, size.height));
            if response.clicked() {
                for window in &group.windows {
                    action_callback(WindowAction::Resize(window.handle, size));
                }
            }
        }

        if let Some(selector) = rule.resize.selector() {
            let selected =
                ComboBox::from_id_salt("resize-all")
                    .width(ui.available_width().min(120.0))
                    .selected_text("RESIZE")
                    .show_ui(ui, |ui| Self::resolution_ui(ui, None, &selector))
                    .inner
                    .flatten();
            if let Some(size) = selected {
                for window in &group.windows {
                    action_callback(WindowAction::Resize(window.handle, size));
                }
            }
        }
    }

    fn window_header_ui(ui: &mut Ui, window: &WindowInfo<B::Handle>) {
        ui.label(format!("{CHAR_WINDOW}"));
        match window.state {
            WindowState::Maximized => { ui.weak("[max]"); },
            WindowState::Minimized => { ui.weak("[min]"); },
            WindowState::Normal => {},
        }
        ui.add(Label::new(window.title.as_str()).truncate());
    }

    /// Renders mutually exclusive resize modes while preserving per-window control policy.
    fn window_action_ui<F: FnMut(WindowAction<B::Handle>)>(
        ui: &mut Ui,
        window: &WindowInfo<B::Handle>,
        rule: &Rule,
        action_callback: &mut F) {
        let detail =
            window
                .detail
                .as_ref()
                .expect("window detail should always present in a rendered snapshot");

        if rule.relocate {
            ui.weak("POSITION");

            let centered =
                detail.monitor_rect.center() ==
                detail.content_rect.center();
            let centered_icon =
                if centered { CHAR_CHECK } else { CHAR_EMPTY };
            ui.add_enabled_ui(!centered, |ui| {
                Button::new(format!("{centered_icon} CENTER"))
                    .pipe(|button| ui.add_sized((80.0, 16.0), button))
                    .clicked()
                    .then(|| action_callback(WindowAction::MoveToCenter(window.handle)));
            });
        } else {
            ui.weak("MOVE DISABLED");
        }

        ui.label("|");


        // Exact mode is exclusive in the authored enum. Selector defaults retain
        // this view's bounds gate; the group primary target is independent of it.
        if let ResizeRule::Exact { exact } = rule.resize {
            let size = Size2D::from(exact);
            ui.weak("SIZE");

            Button::new(format!("{}x{}", size.width, size.height))
                .pipe(|button| ui.add_sized((80.0, 16.0), button))
                .clicked()
                .then(|| action_callback(WindowAction::Resize(window.handle, size)));
        } else if let Some(selector) = rule.resize.selector() {
            ui.weak("SIZE");
            if let Some(size) = selector.default && selector.allows_size(size.into()) {
                Button::new(format!("{}x{}", size[0], size[1]))
                    .pipe(|button| ui.add_sized((80.0, 16.0), button))
                    .clicked()
                    .then(|| action_callback(WindowAction::Resize(window.handle, size.into())));
                ui.weak("OR");
            }
            let selected = ComboBox::from_id_salt("size")
                .width(80.0)
                .selected_text("SELECT")
                .show_ui(ui, |ui| Self::resolution_ui(ui, None, &selector))
                .inner
                .flatten();
            if let Some(size) = selected {
                action_callback(WindowAction::Resize(window.handle, size));
            }
        } else {
            ui.weak("RESIZE DISABLED");
        }
    }

    /// Converts selector clicks into values; native work stays outside egui callbacks.
    fn resolution_ui(
        ui: &mut Ui,
        selected: Option<Size2D<i32>>,
        selector: &ResizeSelector) -> Option<Size2D<i32>> {
        let mut selected_size = None;
        let mut available = false;
        for &(name, resolutions) in STANDARD_SIZE {
            let mut heading_shown = false;
            for size in resolutions.iter().copied().map(Size2D::from)
                .filter(|&size| selector.allows_size(size)) {
                available = true;
                if !heading_shown {
                    ui.add_sized(
                        (ui.available_width(), 0.0),
                        Label::new(RichText::new(format!("-{name}-  ")).weak()));
                    heading_shown = true;
                }
                if ui.selectable_label(
                    selected == Some(size),
                    format!("{}{}{}", size.width, CHAR_CROSS, size.height)).clicked() {
                    selected_size = Some(size);
                    ui.close();
                }
            }
            if heading_shown {
                ui.label("");
            }
        }
        if !available {
            ui.weak("No sizes within configured limits");
        }
        selected_size
    }

    fn size_text(size: Size2D<i32>) -> String {
        let marker =
            if STANDARD_SIZE
                .iter()
                .any(|&(_, sizes)| sizes.contains(&[size.width, size.height])) {
                CHAR_CHECK
            } else {
                CHAR_EMPTY
            };
        format!("{marker} {}x{}", size.width, size.height)
    }
}

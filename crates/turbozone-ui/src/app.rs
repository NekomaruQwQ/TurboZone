use turbozone_core::*;
use smol_str::{SmolStr, format_smolstr};

use std::time::Duration;
use std::time::Instant;

use tap::prelude::*;

use euclid::default::Size2D;

use eframe::egui;
use egui::*;

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
const UPDATE_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 3);

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
    /// Retry pacing belongs to presentation, including failed capture attempts.
    last_update_completed: Instant,
    window_diagnostics: Vec<WindowDiagnostic<B::Handle>>,
}

/// Presentation text for one failed window, refreshed only after engine updates.
/// Native Display may query live metadata, so repainting must reuse the captured text.
struct WindowDiagnostic<H> {
    handle: H,
    description: SmolStr,
    message: SmolStr,
}

impl<B: Backend> eframe::App for App<B> {
    fn logic(&mut self, _: &Context, _: &mut eframe::Frame) {
        if self.should_update(Instant::now()) {
            self.update();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let mut actions = Vec::new();

        CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    self.app_ui(
                        ui,
                        &mut |target, action| actions.push((target, action)));
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
        let mut app = Self {
            engine: Engine::new(rules, backend),
            last_update_completed: Instant::now(),
            window_diagnostics: Vec::new(),
        };
        app.update();
        app
    }

    /// Uses completion time for both successful captures and failed retry attempts.
    /// Pending actions bypass the cadence; no clock or timer policy enters the backend.
    fn should_update(&self, now: Instant) -> bool {
        self.engine.has_pending_actions() ||
        now.saturating_duration_since(self.last_update_completed) >= UPDATE_INTERVAL
    }

    /// Advances native observations and rebuilds their diagnostic presentation together.
    /// Diagnostic rows sort by captured description independently of backend order.
    /// Repainting reuses the text while widget identity continues to use native handles.
    fn update(&mut self) {
        self.engine.update();
        self.last_update_completed = Instant::now();
        self.window_diagnostics.clear();
        if let Ok(snapshot) = self.engine.snapshot().as_ref() {
            self.window_diagnostics.extend(snapshot.iter().filter_map(|&(handle, ref result)| {
                result.as_ref().err().map(|error| WindowDiagnostic {
                    handle,
                    description: format_smolstr!("{handle}"),
                    message: format_smolstr!("{error:#}"),
                })
            }));
            self.window_diagnostics.sort_by(|a, b| a.description.cmp(&b.description));
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

    pub fn setup_egui(egui: &Context) {
        egui.set_visuals(Visuals::dark());
        egui.style_mut_of(Theme::Dark, Self::setup_style);
        egui.style_mut_of(Theme::Light, Self::setup_style);
    }

    const fn setup_style(style: &mut Style) {
        style.interaction.selectable_labels = false;
    }

    /// Derives a borrowed view for this paint and emits actions without native effects.
    /// Enumeration failure replaces all previous content; per-window failures coexist
    /// with usable groups. Both diagnostic states remain visible when logs are silent.
    fn app_ui<F: FnMut(B::Handle, WindowAction)>(
        &self,
        ui: &mut Ui,
        action_callback: &mut F) {
        let view = match self.engine.view() {
            Ok(view) => view,
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, "Couldn't read windows");
                ui.add(Label::new(format!("{error:#}")).wrap().selectable(true));
                ui.weak("Retrying automatically.");
                return;
            },
        };

        for group in &view.groups {
            let group_id = (group.rule_name.as_str(), group.program.path.as_str());
            let rule = self.engine.rule(group.rule_name)
                .expect("a rendered group must reference an active rule");
            ui.push_id(group_id, |ui| {
                Self::group_ui(ui, group, rule, action_callback);
            });
        }
        if view.groups.is_empty() && view.errors.is_empty() {
            ui.weak("- nothing here -");
        }
        if !self.window_diagnostics.is_empty() {
            if !view.groups.is_empty() {
                ui.separator();
            }
            ui.colored_label(ui.visuals().warn_fg_color,
                format!("Windows with errors ({})", self.window_diagnostics.len()));
            for diagnostic in &self.window_diagnostics {
                // Identity remains stable across title changes and sorting/recovery.
                ui.push_id(("window-error", diagnostic.handle), |ui| {
                    ui.add(Label::new(diagnostic.description.as_str()).wrap().selectable(true));
                    ui.add(Label::new(RichText::new(diagnostic.message.as_str()).weak())
                        .wrap().selectable(true));
                    ui.add_space(4.0);
                });
            }
        }
    }

    /// Uses the stable rule name for headings and queries resize availability on demand.
    fn group_ui<F: FnMut(B::Handle, WindowAction)>(
        ui: &mut Ui,
        group: &WindowGroup<'_, B::Handle>,
        rule: &Rule,
        action_callback: &mut F) {
        ui.horizontal(|ui| {
            ui.heading(group.program.description.as_str());
            ui.add_space(4.0);
            ui.monospace(rule.name.as_str());
        });

        ui.add(Label::new(RichText::new(group.program.path.as_str()).weak()).truncate());
        ui.add_space(4.0);

        if  rule.relocate ||
            rule.resize.selector().is_some() ||
            rule.resize.primary_size().is_some() {
            ui.horizontal(|ui| {
                let is_centered =
                    group
                        .windows
                        .iter()
                        .all(|window| window.is_centered());
                let common_size =
                    rule.resize
                        .primary_size()
                        .is_some_and(|primary_size| {
                            group
                                .windows
                                .iter()
                                .map(|window| window.content_rect.size)
                                .all(|size| size == primary_size)
                        });
                Self::action_ui(
                    ui,
                    rule,
                    is_centered,
                    common_size,
                    &mut |action| {
                        for window in &group.windows {
                            action_callback(window.handle, action);
                        }
                    });
            });
            ui.add_space(4.0);
        }

        for window in &group.windows {
            ui.push_id(window.handle, |ui| {
                Self::window_ui(ui, rule, window, &mut |action| {
                    action_callback(window.handle, action);
                });
            });
            ui.add_space(2.0);
        }
        ui.add_space(8.0);
    }

    fn window_ui<F: FnMut(WindowAction)>(
        ui: &mut Ui,
        rule: &Rule,
        window: &WindowInfo<B::Handle>,
        action_callback: &mut F) {
        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.label(format!("{CHAR_WINDOW}"));
            match window.state {
                WindowState::Maximized => { ui.weak("[max]"); },
                WindowState::Minimized => { ui.weak("[min]"); },
                WindowState::Normal => {},
            }
            ui.add(Label::new(window.title.as_str()).truncate());
        });

        ui.horizontal(|ui| {
            let resized =
                rule.resize
                    .primary_size()
                    .map(|size| size == window.content_rect.size)
                    .unwrap_or(true);
            Self::action_ui(
                ui,
                rule,
                window.is_centered(),
                resized,
                action_callback);
        });
    }

    fn action_ui<F: FnMut(WindowAction)>(
        ui: &mut Ui,
        rule: &Rule,
        centered: bool,
        resized: bool,
        action_callback: &mut F) {
        if rule.relocate {
            ui.weak("POSITION");

            let icon =
                if centered {
                    CHAR_CHECK
                } else {
                    CHAR_EMPTY
                };
            ui.add_enabled_ui(!centered, |ui| {
                Button::new(format!("{icon} CENTER"))
                    .pipe(|button| ui.add_sized((80.0, 16.0), button))
                    .clicked()
                    .then(|| action_callback(WindowAction::Center));
            });
        } else {
            ui.weak("MOVE DISABLED");
        }

        ui.label("|");

        // Core owns primary-target eligibility, so group and individual controls both
        // suppress out-of-bounds defaults while retaining the bounded size selector.
        if let ResizeRule::Exact { exact } = rule.resize {
            ui.weak("SIZE");

            let target_size = exact;
            let icon = if resized { CHAR_CHECK } else { CHAR_EMPTY };
            ui.add_enabled_ui(!resized, |ui| {
                Button::new(
                    format!(
                        "{icon} {}x{}",
                        target_size.width,
                        target_size.height))
                    .pipe(|button| ui.add_sized((80.0, 16.0), button))
                    .clicked()
                    .then(|| action_callback(WindowAction::Resize(target_size)));
            });
        } else if let Some(selector) = rule.resize.selector() {
            ui.weak("SIZE");

            if let Some(target_size) = rule.resize.primary_size() {
                let icon = if resized { CHAR_CHECK } else { CHAR_EMPTY };
                ui.add_enabled_ui(!resized, |ui| {
                    Button::new(format!("{icon} {}x{}", target_size.width, target_size.height))
                        .pipe(|button| ui.add_sized((80.0, 16.0), button))
                        .clicked()
                        .then(|| action_callback(WindowAction::Resize(target_size)));
                });
                ui.weak("OR");
            }

            let selected = ComboBox::from_id_salt("size")
                .width(80.0)
                .selected_text("SELECT")
                .show_ui(ui, |ui| Self::size_select_ui(ui, None, &selector))
                .inner
                .flatten();
            if let Some(size) = selected {
                action_callback(WindowAction::Resize(size));
            }
        } else {
            ui.weak("RESIZE DISABLED");
        }
    }

    /// Converts selector clicks into values; native work stays outside egui callbacks.
    fn size_select_ui(
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
}

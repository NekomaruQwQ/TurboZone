//! Eframe scheduling and rendering over a backend-generic core engine.

use std::time::{Duration, Instant};

use eframe::egui::{Context, Ui};
use turbozone_core::{
    Backend, Engine, LOGIC_TICKS_PER_SECOND, RENDER_FRAMES_PER_SECOND, RuntimeConfig,
};

use crate::ui;

/// Keeps painting independent from native query frequency.
const RENDER_INTERVAL: Duration = Duration::from_nanos(
    1_000_000_000 / RENDER_FRAMES_PER_SECOND as u64);
/// Refreshes native details often enough to reflect external window changes.
const LOGIC_INTERVAL: Duration = Duration::from_nanos(
    1_000_000_000 / LOGIC_TICKS_PER_SECOND as u64);

/// TurboZone application state shared by the logic and UI phases.
///
/// The generic backend is owned by core's engine. This layer records only framework
/// timing and translates UI responses into queued core actions.
pub struct App<B: Backend> {
    engine: Engine<B>,
    /// Completion time of the preceding tick; `None` makes startup refresh immediately.
    last_logic_tick: Option<Instant>,
}

impl<B: Backend> App<B> {
    /// Combines validated rules with the platform adapter without native work.
    pub fn new(config: RuntimeConfig, backend: B) -> Self {
        Self {
            engine: Engine::new(config, backend),
            last_logic_tick: None,
        }
    }

    /// Renders against one immutable snapshot, then queues all accepted actions.
    fn app_ui(&mut self, ui: &mut Ui) {
        let actions = ui::app_ui(ui, self.engine.sections(), self.engine.config());
        for action in actions {
            self.engine.queue(action);
        }
    }
}

impl<B: Backend> eframe::App for App<B> {
    fn logic(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        let now = Instant::now();
        let elapsed = self.last_logic_tick
            .map_or(LOGIC_INTERVAL, |last| now.saturating_duration_since(last));
        if self.engine.has_pending_actions() || elapsed >= LOGIC_INTERVAL {
            self.engine.tick();
            self.last_logic_tick = Some(Instant::now());
        }
        let until_tick = self.last_logic_tick
            .map_or(Duration::ZERO, |last| {
                LOGIC_INTERVAL.saturating_sub(last.elapsed())
            });
        ctx.request_repaint_after(until_tick);
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        self.app_ui(ui);
        ui.request_repaint_after(RENDER_INTERVAL);
    }
}

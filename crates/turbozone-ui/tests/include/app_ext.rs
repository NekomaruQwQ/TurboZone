//! Test-only access to the real App rendering and scheduling boundaries.

use super::{App, Backend, Instant, Ui, WindowAction};

trait AppTestExt<B: Backend> {
    fn render<F: FnMut(B::Handle, WindowAction)>(&self, ui: &mut Ui, callback: &mut F);
    fn refresh(&mut self);
    fn update_due(&self, now: Instant) -> bool;
    fn completed_at(&self) -> Instant;
    fn queue_action(&mut self, target: B::Handle, action: WindowAction);
}

impl<B: Backend> AppTestExt<B> for App<B> {
    fn render<F: FnMut(B::Handle, WindowAction)>(&self, ui: &mut Ui, callback: &mut F) {
        self.app_ui(ui, callback);
    }
    fn refresh(&mut self) { self.update(); }
    fn update_due(&self, now: Instant) -> bool { self.should_update(now) }
    fn completed_at(&self) -> Instant { self.last_update_completed }
    fn queue_action(&mut self, target: B::Handle, action: WindowAction) {
        self.engine.queue([(target, action)]);
    }
}

#[path = "../unit/ui.rs"]
mod cases;

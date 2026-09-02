//! Reusable card primitives that preserve TurboZone's visual hierarchy.

use eframe::egui::collapsing_header::CollapsingState;
use eframe::egui::*;

/// Full-width card surface shared by static and collapsible content.
#[derive(Debug, Clone, Copy)]
pub struct Card {
    padding: Margin,
}

impl Card {
    /// Creates a comfortably padded vertical card.
    pub const fn default() -> Self {
        Self {
            padding: Margin::same(8),
        }
    }

    /// Renders a static full-width card.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
        let response = Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6.0)
            .inner_margin(self.padding)
            .show(ui, |ui| {
                ui.take_available_width();
                content(ui)
            });
        ui.add_space(6.0);
        response.inner
    }

    /// Renders a single-surface collapsible card with a custom header and body.
    pub fn show_collapsible<Header, Body>(
        self,
        ui: &mut Ui,
        id_source: impl std::hash::Hash + std::fmt::Debug,
        header: impl FnOnce(&mut Ui) -> Header,
        body: impl FnOnce(&mut Ui) -> Body) -> (Header, Option<Body>) {
        self.show(ui, |ui| {
            let state = CollapsingState::load_with_default_open(
                ui.ctx(),
                ui.make_persistent_id(id_source),
                true);
            let (_, header_response, body_response) = state
                .show_header(ui, header)
                .body_unindented(|ui| {
                    ui.add_space(4.0);
                    ui.add(Separator);
                    ui.add_space(4.0);
                    body(ui)
                });
            (header_response.inner, body_response.map(|response| response.inner))
        })
    }
}

/// One-pixel separator limited to the current card width.
pub struct Separator;

impl Widget for Separator {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 1.0),
            Sense::hover());
        ui.painter().hline(
            rect.x_range(),
            rect.center().y,
            ui.visuals().widgets.noninteractive.bg_stroke);
        response
    }
}

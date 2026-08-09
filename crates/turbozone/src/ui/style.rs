use eframe::egui::*;

use super::color;

const CORNER_RADIUS: CornerRadius = CornerRadius::same(4);

/// Applies the TurboRun-derived dark card theme to the egui context.
pub fn setup_style(context: &Context) {
    context.global_style_mut(|style| {
        let visuals = &mut style.visuals;
        visuals.panel_fill = color::BACKGROUND;
        visuals.window_fill = color::BACKGROUND_ALT;
        visuals.faint_bg_color = color::CARD;
        visuals.extreme_bg_color = color::BACKGROUND;
        visuals.hyperlink_color = color::PRIMARY;
        visuals.window_stroke = Stroke::NONE;
        visuals.window_corner_radius = CornerRadius::same(8);
        visuals.menu_corner_radius = CornerRadius::same(8);
        visuals.override_text_color = Some(color::FOREGROUND);
        visuals.selection.bg_fill = color::PRIMARY.linear_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0, color::PRIMARY);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, color::BORDER);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, color::FOREGROUND);
        widgets.noninteractive.weak_bg_fill = color::BACKGROUND_ALT;
        widgets.inactive.bg_fill = color::CARD;
        widgets.inactive.weak_bg_fill = color::CARD;
        widgets.inactive.bg_stroke = Stroke::NONE;
        widgets.inactive.corner_radius = CORNER_RADIUS;
        widgets.inactive.expansion = 0.0;
        widgets.hovered.bg_fill = color::CARD_HOVER;
        widgets.hovered.weak_bg_fill = color::CARD_HOVER;
        widgets.hovered.bg_stroke = Stroke::NONE;
        widgets.hovered.corner_radius = CORNER_RADIUS;
        widgets.hovered.expansion = 0.0;
        widgets.active.bg_fill = color::CARD_ACTIVE;
        widgets.active.weak_bg_fill = color::CARD_ACTIVE;
        widgets.active.bg_stroke = Stroke::NONE;
        widgets.active.corner_radius = CORNER_RADIUS;
        widgets.active.expansion = 0.0;
        widgets.open.bg_fill = color::CARD_ACTIVE;
        widgets.open.weak_bg_fill = color::CARD_ACTIVE;
        widgets.open.bg_stroke = Stroke::NONE;
        widgets.open.corner_radius = CORNER_RADIUS;
        widgets.open.expansion = 0.0;

        style.interaction.selectable_labels = false;
        style.animation_time = 0.0;
        style.spacing.interact_size = vec2(40.0, 24.0);
        style.spacing.item_spacing = vec2(6.0, 6.0);
        style.spacing.button_padding = vec2(8.0, 4.0);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(11.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
        ].into();
    });
}

#![feature(try_blocks)]

mod app;
mod config;
mod core;
mod native;

fn main() -> eframe::Result {
    use egui::*;
    use eframe::*;

    pretty_env_logger::init();

    eframe::run_native(
        "Nekomaru RnR",
        NativeOptions {
            viewport:
                ViewportBuilder::default()
                    .with_inner_size((360.0, 540.0))
                    .with_resizable(false)
                    .with_maximize_button(false),
            centered: true,
            ..NativeOptions::default()
        },
        Box::new(|cc| {
            let egui = &cc.egui_ctx;
            setup_fonts(egui);

            egui.set_visuals(Visuals::dark());
            egui.style_mut(|style| {
                style.interaction.selectable_labels = false;
            });

            Ok(Box::new(app::App::new()))
        }))
}

fn setup_fonts(egui: &egui::Context) {
    use std::fs;
    use std::sync::Arc;
    use egui::epaint::text::*;

    // Load Microsoft YaHei UI for CJK character support.
    // msyh.ttc index 1 = Microsoft YaHei UI (UI-optimized variant).
    let font_bytes =
        fs::read("C:/Windows/Fonts/msyh.ttc")
            .expect("Failed to read Microsoft YaHei UI font (msyh.ttc)");
    let mut font_data = FontData::from_owned(font_bytes);
    font_data.index = 1;
    let font_data = Arc::new(font_data);

    let mut fonts = FontDefinitions::default();
    fonts.font_data
        .insert("msyahei_ui".to_owned(), font_data);
    // Primary proportional font — CJK + Latin.
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("msyahei_ui".to_owned());

    egui.set_fonts(fonts);
}


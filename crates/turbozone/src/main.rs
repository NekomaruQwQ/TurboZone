mod app;
mod configuration;
mod data;
mod ui;

fn main() -> eframe::Result {
    use eframe::egui::{FontData, FontDefinitions, FontFamily, ViewportBuilder};
    use eframe::NativeOptions;
    use std::sync::Arc;

    pretty_env_logger::init();

    eframe::run_native(
        "TurboRnR",
        NativeOptions {
            viewport: ViewportBuilder::default()
                .with_inner_size((720.0, 680.0))
                .with_min_inner_size((560.0, 420.0)),
            centered: true,
            ..NativeOptions::default()
        },
        Box::new(|creation_context| {
            let egui = &creation_context.egui_ctx;
            let mut fonts = FontDefinitions::default();
            match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
                Ok(bytes) => {
                    let mut font = FontData::from_owned(bytes);
                    font.index = 1;
                    fonts.font_data.insert("msyahei_ui".to_owned(), Arc::new(font));
                    fonts.families
                        .entry(FontFamily::Proportional)
                        .or_default()
                        .push("msyahei_ui".to_owned());
                    egui.set_fonts(fonts);
                },
                Err(error) => {
                    log::warn!("Microsoft YaHei UI font could not be loaded: {error}");
                },
            }
            ui::setup_style(egui);
            Ok(Box::new(app::App::new()))
        }))
}

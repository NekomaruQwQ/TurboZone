//! Entry point of TurboZone for Windows.

use tap::prelude::*;

use eframe::NativeOptions;
use eframe::egui;
use egui::*;

type App = turbozone_ui::app::App<turbozone_windows::Backend>;

fn main() {
    pretty_env_logger::init();

    let config_path =
        dirs::config_local_dir()
            .expect("failed to determine config directory")
            .join("NekomaruQwQ")
            .join("TurboZone")
            .join("config.toml");
    let config =
        turbozone_ui::config::load_config(&config_path)
            .expect("failed to load configuration file");
    let options =
        NativeOptions {
            viewport: App::viewport(),
            centered: true,
            ..NativeOptions::default()
        };
    let result =
        eframe::run_native("TurboZone", options, Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            App::setup_egui(&cc.egui_ctx);
            Ok(Box::new(App::new(config, <_>::default())))
        }));

    result.expect("failed to start eframe application");
}

fn setup_fonts(egui: &Context) {
    use std::sync::Arc;
    let mut fonts = FontDefinitions::default();

    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(bytes) => {
            fonts
                .font_data
                .insert(
                    String::from("msyahei_ui"),
                    FontData::from_owned(bytes)
                        .tap_mut(|font| font.index = 1)
                        .pipe(Arc::new));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("msyahei_ui".to_owned());

        },
        Err(err) =>
            log::warn!("failed to load font 'Microsoft YaHei UI': {err}"),
    }

    egui.set_fonts(fonts);
}

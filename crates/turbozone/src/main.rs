//! Entry point of TurboZone for Windows.

use turbozone_windows::Backend;
use turbozone_ui::app::App as AppBase;
use turbozone_ui::config::load_config;

use eframe::NativeOptions;
use eframe::egui;
use egui::*;

type App = AppBase<Backend>;

fn main() {
    pretty_env_logger::init();

    let config_path =
        dirs::config_local_dir()
            .expect("failed to determine config directory")
            .join("NekomaruQwQ")
            .join("TurboZone")
            .join("config.toml");
    let config =
        load_config(&config_path)
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
            cc.egui_ctx.set_visuals(Visuals::dark());
            cc.egui_ctx.style_mut_of(Theme::Light, setup_style);
            cc.egui_ctx.style_mut_of(Theme::Dark, setup_style);

            Ok(Box::new(App::new(config, Backend::default())))
        }));

    result.expect("failed to start eframe application");
}

const fn setup_style(style: &mut Style) {
    style.interaction.selectable_labels = false;
}

fn setup_fonts(egui: &Context) {
    use std::sync::Arc;

    let mut fonts = FontDefinitions::default();
    App::setup_icon_font(&mut fonts);

    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(bytes) => {
            let mut font = FontData::from_owned(bytes);
            font.index = 1;

            fonts.font_data.insert("msyahei_ui".to_owned(), Arc::new(font));
            fonts.families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("msyahei_ui".to_owned());

        },
        Err(error) =>
            log::warn!("Microsoft YaHei UI font could not be loaded: {error}"),
    }

    egui.set_fonts(fonts);
}

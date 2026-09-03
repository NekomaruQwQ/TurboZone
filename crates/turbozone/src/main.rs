//! Entry point of TurboZone for Windows.

use turbozone_core::constants::*;
use turbozone_windows::Backend;
use turbozone_ui::{
    app::App,
    config::load_config,
    ui::setup_style,
};

use std::path::PathBuf;

use clap::Parser;

use eframe::NativeOptions;
use eframe::egui;
use egui::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Parser)]
#[command(version, about = "Rule-driven window positioning and resizing")]
pub struct Args {
    #[arg(
        short,
        long,
        env = "TURBOZONE_CONFIG",
        value_name = "FILE",
        hide_env_values = true)]
    pub config: PathBuf,
}

fn main() {
    pretty_env_logger::init();

    let args = Args::parse();
    let config =
        load_config(&args.config)
            .expect("failed to load configuration file");
    let viewport =
        ViewportBuilder::default()
            .with_inner_size(APP_WINDOW_SIZE)
            .with_resizable(false)
            .with_maximize_button(false);
    let options =
        NativeOptions {
            viewport,
            centered: true,
            ..NativeOptions::default()
        };

    let result =
        eframe::run_native(APP_NAME, options, Box::new(move |cc| {
            let egui = &cc.egui_ctx;
            setup_fonts(egui);
            setup_style(egui);
            Ok(Box::new(App::new(config, Backend::default())))
        }));

    result.expect("failed to start eframe application");
}

fn setup_fonts(egui: &Context) {
    use std::sync::Arc;

    match std::fs::read("C:/Windows/Fonts/msyh.ttc") {
        Ok(bytes) => {
            let mut font = FontData::from_owned(bytes);
            font.index = 1;

            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert("msyahei_ui".to_owned(), Arc::new(font));
            fonts.families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("msyahei_ui".to_owned());

            egui.set_fonts(fonts);
        },
        Err(error) =>
            log::warn!("Microsoft YaHei UI font could not be loaded: {error}"),
    }
}

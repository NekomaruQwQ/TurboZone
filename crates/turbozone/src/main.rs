use std::process::ExitCode;

use clap::Parser as _;
use turbozone::{app, config, diagnostics, ui};

/// Handles CLI exits before startup I/O and reports fatal application errors once.
fn main() -> ExitCode {
    let args = config::Args::parse();
    if let Err(error) = diagnostics::init_logging() {
        eprintln!("failed to initialize logging: {error}");
        return ExitCode::FAILURE;
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{error:#}");
            ExitCode::FAILURE
        },
    }
}

/// Loads configuration before creating the GUI, so fatal failures cannot look like an empty app.
fn run(args: &config::Args) -> anyhow::Result<()> {
    use eframe::egui::{FontData, FontDefinitions, FontFamily, ViewportBuilder};
    use eframe::NativeOptions;
    use std::sync::Arc;

    let config = config::load_config(&args.config)?;

    eframe::run_native(
        "TurboZone",
        NativeOptions {
            viewport: ViewportBuilder::default()
                .with_inner_size((720.0, 680.0))
                .with_min_inner_size((560.0, 420.0)),
            centered: true,
            ..NativeOptions::default()
        },
        Box::new(move |creation_context| {
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
            Ok(Box::new(app::App::new(config)))
        })).map_err(|error| anyhow::anyhow!("application failed: {error}"))
}

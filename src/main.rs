#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adb;
mod app;
mod parsers;
mod publish;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([980.0, 600.0])
            .with_title("ADB Wizard — монитор устройства"),
        ..Default::default()
    };

    eframe::run_native(
        "ADB Wizard",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

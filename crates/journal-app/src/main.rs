//! Oberfläche des Trainingstagebuchs (Spezifikation Kap. 5–7).

// Unter Windows im Release-Build kein Konsolenfenster öffnen (Kap. 12.5).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod kalender;

use eframe::egui;

fn main() -> eframe::Result {
    let start = app::Start::laden();
    let optionen = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tagebuch")
            .with_inner_size([500.0, 600.0])
            .with_min_inner_size([420.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Tagebuch",
        optionen,
        Box::new(|cc| Ok(Box::new(app::TagebuchApp::new(&cc.egui_ctx, start)))),
    )
}

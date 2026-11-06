//! Oberfläche des Trainingstagebuchs (Spezifikation Kap. 5–7).

// Unter Windows im Release-Build kein Konsolenfenster öffnen (Kap. 12.5).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(windows)]
mod dpi;
mod kalender;

use eframe::egui;

fn main() -> eframe::Result {
    let start = app::Start::laden();
    let optionen = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tagebuch")
            .with_inner_size([500.0, 600.0])
            .with_min_inner_size([420.0, 520.0])
            // Muss zum Namen der .desktop-Datei passen (Linux, Wayland).
            .with_app_id("trainingstagebuch")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .expect("eingebettetes Icon ist ein gültiges PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Tagebuch",
        optionen,
        Box::new(|cc| {
            #[cfg(windows)]
            dpi::installieren(cc);
            Ok(Box::new(app::TagebuchApp::new(&cc.egui_ctx, start)))
        }),
    )
}

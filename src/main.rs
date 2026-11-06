mod analysis;
mod audio;
mod config;
mod i18n;
mod splitter;
mod ui;

use ui::app::TapeDigizerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TapeDigizer")
            .with_inner_size([960.0, 780.0])
            .with_min_inner_size([820.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TapeDigizer",
        native_options,
        Box::new(|cc| Ok(Box::new(TapeDigizerApp::new(cc)))),
    )
}

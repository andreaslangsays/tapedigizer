mod analysis;
mod audio;
mod config;
mod i18n;
mod project;
mod splitter;
mod ui;

use ui::app::TapeDigizerApp;

fn app_icon() -> egui::IconData {
    #[cfg(target_os = "macos")]
    {
        // When running inside a macOS .app bundle, returning empty IconData allows
        // macOS to natively display the high-resolution AppIcon.icns in the Dock
        // without eframe overriding it with a flat/low-res setApplicationIconImage bitmap.
        if let Ok(exe) = std::env::current_exe() {
            if exe.to_string_lossy().contains(".app/Contents/MacOS") {
                return egui::IconData::default();
            }
        }
    }

    let rgba = include_bytes!("../assets/icon_128.rgba").to_vec();
    egui::IconData {
        rgba,
        width: 128,
        height: 128,
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TapeDigizer Pro — Studio Audio Digitization Console")
            .with_icon(app_icon())
            .with_inner_size([1140.0, 840.0])
            .with_min_inner_size([960.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TapeDigizer",
        native_options,
        Box::new(|cc| Ok(Box::new(TapeDigizerApp::new(cc)))),
    )
}

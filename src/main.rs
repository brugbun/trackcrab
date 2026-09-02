// Release builds are windowed apps, not console apps. Without this a Windows
// build pops a console window alongside the GUI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use trackcrab::{app, icon, store::DataStore};

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let store = match DataStore::discover() {
        Ok(store) => store,
        Err(err) => {
            log::error!("{err}");
            // Better a working app writing beside the binary than no app at all.
            DataStore::at("trackcrab.json")
        }
    };
    log::info!("data file: {}", store.path().display());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([720.0, 460.0])
            .with_title("TrackCrab")
            .with_icon(icon::icon_data())
            // Groups the window correctly in Wayland and X11 taskbars.
            .with_app_id("trackcrab"),
        ..Default::default()
    };

    eframe::run_native(
        "TrackCrab",
        options,
        Box::new(move |_cc| Ok(Box::new(app::App::new(store)))),
    )
}

mod commands;
mod error;
mod inspect;
mod jobs;
mod pdfium;
mod settings;

use std::sync::{Arc, Mutex};

use omnimark_core::formats::{default_registry, RegistryConfig};
use tauri::Manager;

pub struct AppState {
    pub registry: Arc<omnimark_core::Registry>,
    pub settings: Mutex<settings::Settings>,
    pub settings_path: std::path::PathBuf,
    pub running: Mutex<Option<omnimark_core::CancelToken>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let default_out = app.path().document_dir()?.join("OmniMark");
            let settings_path = config_dir.join("settings.json");
            let settings = settings::Settings::load(&settings_path, &default_out);
            app.asset_protocol_scope()
                .allow_directory(&settings.output_dir, true)?;
            let pdfium = pdfium::locate(app.path().resource_dir().ok().as_deref());
            if pdfium.is_none() {
                log::warn!("pdfium library not found; PDF conversion is unavailable");
            }
            app.manage(AppState {
                registry: Arc::new(default_registry(&RegistryConfig {
                    pdfium_library: pdfium,
                })),
                settings: Mutex::new(settings),
                settings_path,
                running: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::inspect_files,
            commands::convert,
            commands::cancel,
            commands::read_output,
            commands::open_output_dir,
            commands::reveal,
            commands::open_log_dir,
            commands::get_settings,
            commands::set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running OmniMark");
}

mod commands;
mod error;
mod inspect;
mod jobs;
mod pdfium;
mod settings;
mod webview_mem;

use std::sync::{Arc, Mutex};

use omnimark_core::formats::{default_registry, RegistryConfig};
use tauri::Manager;

pub struct AppState {
    pub registry: Arc<omnimark_core::Registry>,
    pub settings: Mutex<settings::Settings>,
    pub settings_path: std::path::PathBuf,
    pub default_output_dir: std::path::PathBuf,
    pub running: Mutex<Option<omnimark_core::CancelToken>>,
    /// Where downloaded OCR models live (`<app local data>/models`).
    pub models_root: std::path::PathBuf,
    pub downloading: Mutex<Option<omnimark_core::CancelToken>>,
}

/// Dependencies log at Warn and the app at Info. The HTML parser crates are silenced: at
/// Trace they log every character of the converted documents.
fn log_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    use log::LevelFilter;
    tauri_plugin_log::Builder::new()
        .max_file_size(2_000_000)
        .level(LevelFilter::Warn)
        .level_for("omnimark_app_lib", LevelFilter::Info)
        .level_for("omnimark_core", LevelFilter::Info)
        .level_for("html5ever", LevelFilter::Off)
        .level_for("markup5ever", LevelFilter::Off)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let default_out = settings::default_output_dir(
                app.path().document_dir().ok(),
                app.path().home_dir().ok(),
                app.path().app_data_dir().ok(),
            )
            .ok_or("no folder is available for converted files")?;
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
                default_output_dir: default_out,
                running: Mutex::new(None),
                models_root: app.path().app_local_data_dir()?.join("models"),
                downloading: Mutex::new(None),
            });
            webview_mem::build_main_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::inspect_files,
            commands::convert,
            commands::cancel,
            commands::read_output,
            commands::open_output_dir,
            commands::reveal,
            commands::open_output,
            commands::trim_memory,
            commands::open_log_dir,
            commands::get_settings,
            commands::set_settings,
            commands::ocr_status,
            commands::download_ocr_models,
            commands::cancel_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running OmniMark");
}

//! Tauri commands: the only surface the frontend can call.

use std::io::Read;
use std::path::{Path, PathBuf};

use omnimark_core::{CancelToken, ConvertOptions};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::AppError;
use crate::inspect::{inspect, InspectResult};
use crate::jobs::{run_batch, BatchRequest, ConvertEvent};
use crate::settings::Settings;
use crate::AppState;

/// Largest dropped selection (files after folder expansion) handled in one go.
const INSPECT_LIMIT: usize = 1000;
/// Largest preview read from an output file.
const PREVIEW_LIMIT: usize = 2 * 1024 * 1024;

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn current_settings(state: &AppState) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command]
pub async fn inspect_files(
    state: State<'_, AppState>,
    paths: Vec<PathBuf>,
) -> Result<InspectResult, AppError> {
    let registry = state.registry.clone();
    tauri::async_runtime::spawn_blocking(move || inspect(&registry, &paths, INSPECT_LIMIT))
        .await
        .map_err(|e| AppError::Tauri(e.to_string()))
}

/// Clears the "running" slot when the batch ends, even if the worker thread panics.
struct RunningGuard<'a>(&'a AppState);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        *lock(&self.0.running) = None;
    }
}

#[tauri::command]
pub async fn convert(
    state: State<'_, AppState>,
    files: Vec<PathBuf>,
    on_event: Channel<ConvertEvent>,
) -> Result<(), AppError> {
    let cancel = CancelToken::new();
    {
        let mut running = lock(&state.running);
        if running.is_some() {
            return Err(AppError::Busy);
        }
        *running = Some(cancel.clone());
    }
    let _guard = RunningGuard(&state);

    let settings = current_settings(&state);
    let request = BatchRequest {
        files,
        output_dir: settings.output_dir,
        options: ConvertOptions {
            extract_images: settings.extract_images,
            ..ConvertOptions::default()
        },
        workers: settings.workers,
    };
    let registry = state.registry.clone();
    tauri::async_runtime::spawn_blocking(move || {
        run_batch(&registry, &request, &cancel, &|event| {
            let _ = on_event.send(event);
        });
    })
    .await
    .map_err(|e| AppError::Tauri(e.to_string()))
}

#[tauri::command]
pub fn cancel(state: State<'_, AppState>) {
    if let Some(token) = lock(&state.running).as_ref() {
        token.cancel();
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputText {
    pub text: String,
    pub truncated: bool,
}

/// Reads at most `limit` bytes, cutting back to a UTF-8 boundary.
fn read_limited(path: &Path, limit: usize) -> std::io::Result<OutputText> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > limit;
    bytes.truncate(limit);
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            let valid = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            if truncated {
                bytes.truncate(valid);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };
    Ok(OutputText { text, truncated })
}

#[tauri::command]
pub fn read_output(state: State<'_, AppState>, path: PathBuf) -> Result<OutputText, AppError> {
    let output_dir = current_settings(&state).output_dir;
    let root = output_dir
        .canonicalize()
        .map_err(|_| AppError::InvalidPath(path.display().to_string()))?;
    let file = path
        .canonicalize()
        .map_err(|_| AppError::InvalidPath(path.display().to_string()))?;
    let is_markdown = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if !file.starts_with(&root) || !is_markdown {
        return Err(AppError::InvalidPath(path.display().to_string()));
    }
    Ok(read_limited(&file, PREVIEW_LIMIT)?)
}

fn open_dir(app: &AppHandle, dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| AppError::Tauri(e.to_string()))
}

#[tauri::command]
pub fn open_output_dir(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    open_dir(&app, &current_settings(&state).output_dir)
}

#[tauri::command]
pub fn reveal(app: AppHandle, path: PathBuf) -> Result<(), AppError> {
    let path = path
        .canonicalize()
        .map_err(|_| AppError::InvalidPath(path.display().to_string()))?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| AppError::Tauri(e.to_string()))
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), AppError> {
    open_dir(&app, &app.path().app_log_dir()?)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    current_settings(&state)
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let settings = settings.sanitized();
    settings.save(&state.settings_path)?;
    app.asset_protocol_scope()
        .allow_directory(&settings.output_dir, true)?;
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_limited_cuts_at_a_utf8_boundary_and_flags_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "aéé").unwrap(); // 5 bytes
        let cut = read_limited(&path, 2).unwrap();
        assert_eq!(
            cut,
            OutputText {
                text: "a".into(),
                truncated: true
            }
        );
        let whole = read_limited(&path, 5).unwrap();
        assert_eq!(
            whole,
            OutputText {
                text: "aéé".into(),
                truncated: false
            }
        );
    }
}

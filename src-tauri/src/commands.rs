//! Tauri commands: the only surface the frontend can call.

use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use omnimark_core::ocr::models::{fetch_ocrs, ocrs_download_size, DownloadError};
use omnimark_core::ocr::{availability, build_engine, select, Availability, EngineKind, OcrChoice};
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
/// Largest preview read from an output file (bytes). Rendering more costs hundreds of MB in WebView2.
const PREVIEW_LIMIT: usize = 64 * 1024;
/// Most headings returned for the outline of a document.
const MAX_HEADINGS: usize = 500;

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
    let models_root = state.models_root.clone();
    let registry = state.registry.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut options = ConvertOptions {
            extract_images: settings.extract_images,
            ..ConvertOptions::default()
        };
        let mut ocr_warning = None;
        // One engine per batch, built here (never at startup) and dropped with `request`.
        if settings.ocr.enabled {
            let config = settings.engine_config(&models_root);
            match build_engine(OcrChoice::Auto, &config) {
                Ok(engine) => options.ocr = Some(engine),
                Err(e) => ocr_warning = Some(format!("OCR was skipped: {e}.")),
            }
        }
        let request = BatchRequest {
            files,
            output_dir: settings.output_dir,
            options,
            workers: settings.workers,
            ocr_warning,
            zip_output: settings.zip_output,
        };
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
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// True when the heading line lies entirely inside the returned preview text.
    pub in_preview: bool,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputText {
    pub text: String,
    pub truncated: bool,
    pub total_bytes: u64,
    /// Exact length in bytes of the returned text.
    pub shown_bytes: u64,
    pub headings: Vec<Heading>,
    pub headings_truncated: bool,
}

/// Longest line the outline scan keeps in memory; longer lines are skipped (never a heading).
const MAX_LINE: usize = 64 * 1024;

/// Where to cut a window that is known to be cut short (the file continues past it). The cut
/// lands on a UTF-8 boundary, then on the last blank line (`\n\n` or `\r\n\r\n`) when that
/// keeps at least half of the window, else on the last line break (before its `\r`), else on
/// the boundary itself. `bytes` should hold at least one byte past `limit`.
fn preview_cut(bytes: &[u8], limit: usize) -> usize {
    let mut end = limit.min(bytes.len());
    // A UTF-8 continuation byte (10xxxxxx) at `end` means `end` is inside a character.
    while end > 0 && end < bytes.len() && bytes[end] & 0xC0 == 0x80 {
        end -= 1;
    }
    let window = &bytes[..end];
    let blank = rfind(window, b"\n\n").max(rfind(window, b"\r\n\r\n"));
    if let Some(i) = blank.filter(|&i| i > 0 && i >= end / 2) {
        return i;
    }
    match rfind(window, b"\n") {
        Some(i) if i > 0 => {
            if window[i - 1] == b'\r' {
                i - 1
            } else {
                i
            }
        }
        _ => end,
    }
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// ATX heading of a single line: 0-3 spaces, 1-6 `#`, a space, text. `None` for anything else.
fn parse_atx(line: &str) -> Option<(u8, String)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let body = rest[hashes..].strip_prefix([' ', '\t'])?.trim();
    // A closing run of `#` only counts when it is separated from the text by whitespace.
    let without_closing = body.trim_end_matches('#');
    let text = if without_closing.is_empty() {
        ""
    } else if without_closing.ends_with([' ', '\t']) {
        without_closing.trim_end()
    } else {
        body
    };
    if text.is_empty() {
        return None;
    }
    Some((hashes as u8, text.to_string()))
}

/// Opening or closing code fence: (fence character, run length, text after the run).
fn fence_of(line: &str) -> Option<(u8, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|&b| b == ch).count();
    (run >= 3).then(|| (ch, run, &rest[run..]))
}

/// Skips input up to and including the next `\n`; returns the bytes skipped.
fn skip_line(reader: &mut impl std::io::BufRead) -> std::io::Result<u64> {
    let mut skipped = 0u64;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(skipped);
        }
        match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => {
                reader.consume(i + 1);
                return Ok(skipped + i as u64 + 1);
            }
            None => {
                let n = chunk.len();
                reader.consume(n);
                skipped += n as u64;
            }
        }
    }
}

/// Scans a whole document line by line and collects ATX headings outside fenced code blocks.
/// A heading is `in_preview` when its text ends at or before byte `preview_end`. Stops after
/// `max` headings and reports whether more existed. Memory is bounded: lines longer than
/// `MAX_LINE` are skipped without being stored and count as plain text.
fn extract_headings(
    mut reader: impl std::io::BufRead,
    preview_end: u64,
    max: usize,
) -> std::io::Result<(Vec<Heading>, bool)> {
    let mut headings = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut buf = Vec::new();
    let mut offset = 0u64;
    loop {
        buf.clear();
        let read = (&mut reader)
            .take(MAX_LINE as u64)
            .read_until(b'\n', &mut buf)?;
        if read == 0 {
            return Ok((headings, false));
        }
        let start = offset;
        offset += read as u64;
        if buf.last() != Some(&b'\n') && read == MAX_LINE {
            offset += skip_line(&mut reader)?;
            continue;
        }
        // Raw terminator: `\n`, then `\r`.
        let mut body: &[u8] = &buf;
        if let Some(rest) = body.strip_suffix(b"\n") {
            body = rest;
        }
        if let Some(rest) = body.strip_suffix(b"\r") {
            body = rest;
        }
        let text_end = start + body.len() as u64;
        let line = String::from_utf8_lossy(body);
        if let Some((ch, run, tail)) = fence_of(&line) {
            match fence {
                None if ch != b'`' || !tail.contains('`') => fence = Some((ch, run)),
                Some((open_ch, open_run))
                    if ch == open_ch && run >= open_run && tail.trim().is_empty() =>
                {
                    fence = None
                }
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if let Some((level, text)) = parse_atx(&line) {
            if headings.len() == max {
                return Ok((headings, true));
            }
            headings.push(Heading {
                level,
                text,
                in_preview: text_end <= preview_end,
            });
        }
    }
}

/// Reads the first `limit` bytes of `path` for display (cut at a paragraph boundary when the
/// file is longer) and outlines the whole document.
fn read_limited(path: &Path, limit: usize) -> std::io::Result<OutputText> {
    let total_bytes = std::fs::metadata(path)?.len();
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > limit;
    let cut = if truncated {
        preview_cut(&bytes, limit)
    } else {
        bytes.len()
    };
    bytes.truncate(cut);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let (headings, headings_truncated) = extract_headings(reader, cut as u64, MAX_HEADINGS)?;
    Ok(OutputText {
        text,
        truncated,
        total_bytes,
        shown_bytes: cut as u64,
        headings,
        headings_truncated,
    })
}

/// Runs on the blocking pool: the outline scan reads the whole file.
#[tauri::command]
pub async fn read_output(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<OutputText, AppError> {
    let root = current_settings(&state).output_dir;
    tauri::async_runtime::spawn_blocking(move || {
        let file = resolve_output_path(&root, &path)?;
        Ok(read_limited(&file, PREVIEW_LIMIT)?)
    })
    .await
    .map_err(|e| AppError::Tauri(e.to_string()))?
}

/// Canonicalizes `path` and accepts it only if it is an existing `.md` file inside `root`.
fn resolve_output_path(root: &Path, path: &Path) -> Result<PathBuf, AppError> {
    let invalid = || AppError::InvalidPath(path.display().to_string());
    let root = root.canonicalize().map_err(|_| invalid())?;
    let file = path.canonicalize().map_err(|_| invalid())?;
    let is_markdown = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if file.is_file() && is_markdown && file.starts_with(&root) {
        Ok(file)
    } else {
        Err(invalid())
    }
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

/// Opens a produced `.md` in the OS default application (same confinement as `read_output`).
#[tauri::command]
pub fn open_output(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<(), AppError> {
    let file = resolve_output_path(&current_settings(&state).output_dir, &path)?;
    app.opener()
        .open_path(file.display().to_string(), None::<&str>)
        .map_err(|e| AppError::Tauri(e.to_string()))
}

/// Asks WebView2 to re-apply its memory level so a closed preview's memory is returned sooner.
#[tauri::command]
pub fn trim_memory(app: AppHandle) {
    crate::webview_mem::trim(&app);
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
    let settings = settings.sanitized(&state.default_output_dir);
    settings.save(&state.settings_path)?;
    app.asset_protocol_scope()
        .allow_directory(&settings.output_dir, true)?;
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

/// What the settings dialog needs to know about OCR.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatus {
    /// The engine OCR will use; `None` when this build has none.
    pub engine: Option<EngineKind>,
    /// The engine can run now.
    pub ready: bool,
    /// ocrs is the engine and its models are not downloaded yet.
    pub needs_download: bool,
    pub ocrs_download_bytes: u64,
    pub downloading: bool,
}

impl OcrStatus {
    /// Windows OCR when it has a language, else ocrs (which may still need its models).
    fn new(availability: &Availability, ocrs_download_bytes: u64, downloading: bool) -> Self {
        let selected = select(OcrChoice::Auto, availability);
        let engine = selected.or(availability.ocrs_built.then_some(EngineKind::Ocrs));
        Self {
            engine,
            ready: selected.is_some(),
            needs_download: engine == Some(EngineKind::Ocrs) && selected.is_none(),
            ocrs_download_bytes,
            downloading,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum DownloadEvent {
    Progress { downloaded: u64, total: u64 },
    Finished,
    Failed { message: String },
    Cancelled,
}

/// Which engine OCR will use and whether it is ready; asks Windows, so off the UI thread.
#[tauri::command]
pub async fn ocr_status(state: State<'_, AppState>) -> Result<OcrStatus, AppError> {
    let settings = current_settings(&state);
    let models_root = state.models_root.clone();
    let downloading = lock(&state.downloading).is_some();
    tauri::async_runtime::spawn_blocking(move || {
        let availability = availability(&settings.engine_config(&models_root));
        OcrStatus::new(&availability, ocrs_download_size(), downloading)
    })
    .await
    .map_err(|e| AppError::Tauri(e.to_string()))
}

/// Clears the "downloading" slot when the command ends, even on a panic.
struct DownloadGuard<'a>(&'a AppState);

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        *lock(&self.0.downloading) = None;
    }
}

fn download_outcome(result: Result<(), DownloadError>) -> DownloadEvent {
    match result {
        Ok(()) => DownloadEvent::Finished,
        Err(DownloadError::Cancelled) => DownloadEvent::Cancelled,
        Err(e) => DownloadEvent::Failed {
            message: e.to_string(),
        },
    }
}

/// Downloads the ocrs models. The download runs on its own thread; this command returns as
/// soon as the user cancels, without joining it: ureq cannot interrupt a blocked read, so the
/// thread may need until the body timeout to notice. It then deletes its own `.part` file
/// (unique per download), so a new download started meanwhile may safely run concurrently.
#[tauri::command]
pub async fn download_ocr_models(
    state: State<'_, AppState>,
    on_event: Channel<DownloadEvent>,
) -> Result<(), AppError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::RecvTimeoutError;

    let cancel = CancelToken::new();
    {
        let mut downloading = lock(&state.downloading);
        if downloading.is_some() {
            return Err(AppError::Busy);
        }
        *downloading = Some(cancel.clone());
    }
    let _guard = DownloadGuard(&state);

    let models_root = state.models_root.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_cancel = cancel.clone();
    let worker_events = on_event.clone();
    std::thread::spawn(move || {
        let last_percent = AtomicU64::new(u64::MAX);
        let result = fetch_ocrs(&models_root, &worker_cancel, &|p| {
            let percent = (p.downloaded * 100).checked_div(p.total).unwrap_or(0);
            let changed = last_percent.swap(percent, Ordering::Relaxed) != percent;
            if changed && !worker_cancel.is_cancelled() {
                let _ = worker_events.send(DownloadEvent::Progress {
                    downloaded: p.downloaded,
                    total: p.total,
                });
            }
        });
        let _ = tx.send(result);
    });

    let outcome = tauri::async_runtime::spawn_blocking(move || loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(result) => break Some(download_outcome(result)),
            Err(RecvTimeoutError::Timeout) if cancel.is_cancelled() => break None,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break Some(DownloadEvent::Failed {
                    message: "the download stopped unexpectedly".into(),
                })
            }
        }
    })
    .await
    .map_err(|e| AppError::Tauri(e.to_string()))?;
    let _ = on_event.send(outcome.unwrap_or(DownloadEvent::Cancelled));
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>) {
    if let Some(token) = lock(&state.downloading).as_ref() {
        token.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_events_serialize_like_convert_events() {
        assert_eq!(
            serde_json::to_value(DownloadEvent::Progress {
                downloaded: 1,
                total: 2
            })
            .unwrap(),
            serde_json::json!({"event": "progress", "data": {"downloaded": 1, "total": 2}})
        );
        assert_eq!(
            serde_json::to_value(DownloadEvent::Finished).unwrap(),
            serde_json::json!({"event": "finished"})
        );
    }

    fn available(windows: bool, ocrs_built: bool, ocrs_models: bool) -> Availability {
        Availability {
            windows_built: true,
            windows_languages: if windows {
                vec!["en-US".into()]
            } else {
                vec![]
            },
            ocrs_built,
            ocrs_models,
        }
    }

    #[test]
    fn ocr_status_reports_engine_readiness_and_download_need() {
        let windows = OcrStatus::new(&available(true, true, false), 5, false);
        assert_eq!(windows.engine, Some(EngineKind::Windows));
        assert!(windows.ready && !windows.needs_download);
        let missing = OcrStatus::new(&available(false, true, false), 5, false);
        assert_eq!(missing.engine, Some(EngineKind::Ocrs));
        assert!(!missing.ready && missing.needs_download);
        let ocrs = OcrStatus::new(&available(false, true, true), 5, false);
        assert!(ocrs.ready && !ocrs.needs_download);
        let none = OcrStatus::new(&available(false, false, false), 5, false);
        assert_eq!(none.engine, None);
        assert!(!none.ready && !none.needs_download);
        let json = serde_json::to_value(missing).unwrap();
        assert_eq!(json["engine"], "ocrs");
        assert_eq!(json["needsDownload"], true);
        assert_eq!(json["ocrsDownloadBytes"], 5);
    }

    #[test]
    fn download_results_map_to_events() {
        assert_eq!(download_outcome(Ok(())), DownloadEvent::Finished);
        assert_eq!(
            download_outcome(Err(DownloadError::Cancelled)),
            DownloadEvent::Cancelled
        );
        assert!(matches!(
            download_outcome(Err(DownloadError::Network("x".into()))),
            DownloadEvent::Failed { .. }
        ));
    }

    #[test]
    fn resolve_output_path_confines_reads_to_markdown_inside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("out");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/a.md"), "x").unwrap();
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        std::fs::write(tmp.path().join("secret.md"), "x").unwrap();

        let ok = resolve_output_path(&root, &root.join("sub/a.md")).unwrap();
        assert_eq!(ok, root.join("sub/a.md").canonicalize().unwrap());
        // `..` traversal out of the root
        let escape = root.join("sub/../../secret.md");
        assert!(resolve_output_path(&root, &escape).is_err());
        // wrong extension
        assert!(resolve_output_path(&root, &root.join("notes.txt")).is_err());
        // missing root, missing file, directory
        assert!(resolve_output_path(&tmp.path().join("nope"), &root.join("sub/a.md")).is_err());
        assert!(resolve_output_path(&root, &root.join("missing.md")).is_err());
        std::fs::create_dir(root.join("dir.md")).unwrap();
        assert!(resolve_output_path(&root, &root.join("dir.md")).is_err());
    }

    #[test]
    fn resolve_output_path_rejects_symlink_escaping_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("out");
        std::fs::create_dir_all(&root).unwrap();
        let secret = tmp.path().join("secret.md");
        std::fs::write(&secret, "x").unwrap();
        let link = root.join("link.md");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&secret, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&secret, &link);
        if made.is_err() {
            eprintln!("skipped: cannot create symlinks here");
            return;
        }
        assert!(resolve_output_path(&root, &link).is_err());
    }

    fn headings_of_bytes(bytes: &[u8], preview_end: u64, max: usize) -> (Vec<Heading>, bool) {
        extract_headings(std::io::Cursor::new(bytes), preview_end, max).unwrap()
    }

    fn headings_of(text: &str, preview_end: u64, max: usize) -> (Vec<Heading>, bool) {
        headings_of_bytes(text.as_bytes(), preview_end, max)
    }

    fn pairs(headings: &[Heading]) -> Vec<(u8, &str)> {
        headings
            .iter()
            .map(|h| (h.level, h.text.as_str()))
            .collect()
    }

    #[test]
    fn preview_cut_prefers_blank_line_then_newline_then_utf8_boundary() {
        let text = b"para one\n\npara two\nstill two\n\npara three is long";
        // window ends inside "para three": cut at the last blank line before it
        assert_eq!(preview_cut(text, 35), 28);
        // no blank line in the window: last newline
        assert_eq!(preview_cut(b"line one\nline two continues", 15), 8);
        // no newline at all: plain UTF-8 boundary ("aéé" = a, C3 A9, C3 A9)
        assert_eq!(preview_cut("aéé".as_bytes(), 2), 1);
        assert_eq!(preview_cut("aéé".as_bytes(), 3), 3);
        // limit beyond the data never panics
        assert_eq!(preview_cut(b"abc", 10), 3);
    }

    #[test]
    fn preview_cut_handles_crlf_and_keeps_most_of_the_window() {
        // CRLF blank line counts as a blank line; the cut lands before it
        let crlf = b"one\r\n\r\ntwo\r\n\r\nthree is long";
        assert_eq!(preview_cut(crlf, 20), 10);
        // a plain CRLF break is cut before its \r
        assert_eq!(preview_cut(b"line one\r\nline two continues", 18), 8);
        // a blank line in the first half is not worth collapsing to: use the last newline
        let early = b"ab\n\ncccccccc\ndddddddd\neeeeeeee";
        assert_eq!(preview_cut(early, 28), 21);
    }

    #[test]
    fn headings_skip_fenced_code_and_non_headings() {
        let doc = "# One\n#notaheading\n####### seven\n```\n# in fence\n```\n~~~\n## in tilde\n~~~\n## Two ##\n    # indented code\n###### Six\n#\n";
        let (found, capped) = headings_of(doc, u64::MAX, 500);
        assert!(!capped);
        assert_eq!(pairs(&found), vec![(1, "One"), (2, "Two"), (6, "Six")]);
    }

    #[test]
    fn heading_text_is_trimmed_and_closing_hashes_stripped() {
        let (found, _) = headings_of(
            "##   Spaced   \n# C#\n### Title ###   \n# ###\n",
            u64::MAX,
            500,
        );
        assert_eq!(pairs(&found), vec![(2, "Spaced"), (1, "C#"), (3, "Title")]);
    }

    #[test]
    fn a_longer_fence_is_not_closed_by_a_shorter_one() {
        let doc = "````\n```\n# still code\n````\n# real\n";
        let (found, _) = headings_of(doc, u64::MAX, 500);
        assert_eq!(pairs(&found), vec![(1, "real")]);
    }

    #[test]
    fn heading_cap_reports_truncation() {
        let doc = "# a\n# b\n# c\n";
        let (found, capped) = headings_of(doc, u64::MAX, 2);
        assert_eq!(found.len(), 2);
        assert!(capped);
        let (found, capped) = headings_of(doc, u64::MAX, 3);
        assert_eq!(found.len(), 3);
        assert!(!capped);
    }

    #[test]
    fn in_preview_is_true_only_for_lines_inside_the_cut() {
        let doc = "# a\r\n\r\n# b\n";
        // "# a" ends at byte 3, "# b" at byte 11
        let (found, _) = headings_of(doc, 3, 500);
        assert_eq!(
            found.iter().map(|h| h.in_preview).collect::<Vec<_>>(),
            vec![true, false]
        );
        let (found, _) = headings_of(doc, 10, 500);
        assert!(found.iter().all(|h| h.in_preview));
    }

    #[test]
    fn overlong_lines_are_skipped_without_losing_offsets_or_fence_state() {
        let long = "x".repeat(MAX_LINE * 3 + 17);
        // an overlong "heading" is never a heading; the next line's offset stays exact
        let doc = format!("# a\n# {long}\n# b\n");
        let end_of_b = doc.len() as u64 - 1;
        let (found, _) = headings_of(&doc, end_of_b, 500);
        assert_eq!(pairs(&found), vec![(1, "a"), (1, "b")]);
        assert!(found[1].in_preview);
        let (found, _) = headings_of(&doc, end_of_b - 1, 500);
        assert!(!found[1].in_preview);
        // a line of exactly MAX_LINE bytes plus newline is still read normally
        let exact = format!("# {}\n# c\n", "y".repeat(MAX_LINE - 3));
        let (found, _) = headings_of(&exact, u64::MAX, 500);
        assert_eq!(found.len(), 2);
        // fence state survives an overlong line inside the fence
        let fenced = format!("```\n{long}\n# hidden\n```\n# shown\n");
        let (found, _) = headings_of(&fenced, u64::MAX, 500);
        assert_eq!(pairs(&found), vec![(1, "shown")]);
        // overlong last line without a newline
        let tail = format!("# a\n{long}");
        let (found, capped) = headings_of(&tail, u64::MAX, 500);
        assert_eq!(pairs(&found), vec![(1, "a")]);
        assert!(!capped);
    }

    #[test]
    fn invalid_utf8_never_breaks_offsets() {
        // invalid byte at EOF, no trailing newline
        let doc = b"# ok\n# bad\xff";
        let (found, _) = headings_of_bytes(doc, doc.len() as u64, 500);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].text, "bad\u{fffd}");
        assert!(found[1].in_preview);
        // invalid bytes right before a CRLF: raw terminator length is used
        let doc = b"# a\xff\xfe\r\n# b\n";
        let (found, _) = headings_of_bytes(doc, 6, 500);
        assert_eq!(
            found.iter().map(|h| h.in_preview).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn invalid_utf8_at_the_preview_boundary_is_cut_safely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let mut bytes = b"# head\n\nsome text ".to_vec();
        bytes.extend_from_slice(&[0x80; 8]); // stray continuation bytes
        bytes.extend_from_slice(b"\n\n# tail\n");
        std::fs::write(&path, &bytes).unwrap();
        let out = read_limited(&path, 22).unwrap();
        assert!(out.truncated);
        assert_eq!(out.shown_bytes as usize, out.text.len().min(22));
        assert_eq!(pairs(&out.headings), vec![(1, "head"), (1, "tail")]);
        assert!(out.headings[0].in_preview && !out.headings[1].in_preview);
    }

    #[test]
    fn read_limited_returns_whole_small_files_with_outline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# Hi\n\ntext\n").unwrap();
        let out = read_limited(&path, 1024).unwrap();
        assert_eq!(out.text, "# Hi\n\ntext\n");
        assert!(!out.truncated);
        assert_eq!(out.total_bytes, 11);
        assert_eq!(out.shown_bytes, 11);
        assert_eq!(pairs(&out.headings), vec![(1, "Hi")]);
        assert!(out.headings[0].in_preview);
        assert!(!out.headings_truncated);
    }

    #[test]
    fn read_limited_cuts_large_files_at_a_paragraph_and_outlines_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let doc =
            "# First\n\nalpha beta\n\n## Second\n\ngamma delta epsilon zeta\n\n## Late\n\nend\n";
        std::fs::write(&path, doc).unwrap();
        let out = read_limited(&path, 40).unwrap();
        assert!(out.truncated);
        assert_eq!(out.total_bytes, doc.len() as u64);
        assert_eq!(out.shown_bytes as usize, out.text.len());
        assert!(doc.starts_with(&out.text));
        assert!(out.text.ends_with("## Second"), "{:?}", out.text);
        assert_eq!(
            out.headings
                .iter()
                .map(|h| (h.text.as_str(), h.in_preview))
                .collect::<Vec<_>>(),
            vec![("First", true), ("Second", true), ("Late", false)]
        );
    }

    #[test]
    fn read_limited_cuts_at_a_utf8_boundary_without_newlines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "aéé").unwrap(); // 5 bytes
        let cut = read_limited(&path, 2).unwrap();
        assert_eq!(cut.text, "a");
        assert!(cut.truncated);
        let whole = read_limited(&path, 5).unwrap();
        assert_eq!(whole.text, "aéé");
        assert!(!whole.truncated);
    }
}

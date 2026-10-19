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
/// Largest preview read from an output file (bytes). Rendering more costs hundreds of MB in WebView2.
const PREVIEW_LIMIT: usize = 256 * 1024;
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
    pub headings: Vec<Heading>,
    pub headings_truncated: bool,
}

/// Where to cut a window that is known to be cut short (the file continues past it): back to a
/// UTF-8 boundary, then to the last blank line, else the last newline, else the boundary itself.
/// `bytes` must hold at least one byte past `limit` so the boundary test can look at it.
fn preview_cut(bytes: &[u8], limit: usize) -> usize {
    let limit = limit.min(bytes.len());
    let mut end = limit;
    // A UTF-8 continuation byte (10xxxxxx) at `end` means `end` is inside a character.
    while end > 0 && end < bytes.len() && bytes[end] & 0xC0 == 0x80 {
        end -= 1;
    }
    let window = &bytes[..end];
    if let Some(i) = rfind(
        window, b"

",
    ) {
        return i;
    }
    if let Some(i) = rfind(
        window, b"
",
    ) {
        return i;
    }
    end
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

/// Scans a whole document line by line and collects ATX headings outside fenced code blocks.
/// A heading is `in_preview` when its line ends at or before byte `preview_end`. Stops after
/// `max` headings and reports whether more existed.
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
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 {
            return Ok((headings, false));
        }
        let end = offset + read as u64;
        offset = end;
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if let Some((ch, run, tail)) = fence_of(line) {
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
        if let Some((level, text)) = parse_atx(line) {
            if headings.len() == max {
                return Ok((headings, true));
            }
            // The line ends before its newline; `end` includes it, so compare the text end.
            let line_end = end - (read - line.len()) as u64;
            headings.push(Heading {
                level,
                text,
                in_preview: line_end <= preview_end,
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
        headings,
        headings_truncated,
    })
}

#[tauri::command]
pub fn read_output(state: State<'_, AppState>, path: PathBuf) -> Result<OutputText, AppError> {
    let file = resolve_output_path(&current_settings(&state).output_dir, &path)?;
    Ok(read_limited(&file, PREVIEW_LIMIT)?)
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn headings_of(text: &str, preview_end: u64, max: usize) -> (Vec<Heading>, bool) {
        extract_headings(std::io::Cursor::new(text.as_bytes()), preview_end, max).unwrap()
    }

    fn pairs(headings: &[Heading]) -> Vec<(u8, &str)> {
        headings
            .iter()
            .map(|h| (h.level, h.text.as_str()))
            .collect()
    }

    #[test]
    fn preview_cut_prefers_blank_line_then_newline_then_utf8_boundary() {
        let text = b"para one

para two
still two

para three is long";
        // window ends inside "para three": cut at the last blank line before it
        assert_eq!(preview_cut(text, 35), 28);
        // no blank line in the window: last newline
        assert_eq!(
            preview_cut(
                b"line one
line two continues",
                15
            ),
            8
        );
        // no newline at all: plain UTF-8 boundary ("aéé" = a, C3 A9, C3 A9)
        assert_eq!(preview_cut("aéé".as_bytes(), 2), 1);
        assert_eq!(preview_cut("aéé".as_bytes(), 3), 3);
        // limit beyond the data never panics
        assert_eq!(preview_cut(b"abc", 10), 3);
    }

    #[test]
    fn headings_skip_fenced_code_and_non_headings() {
        let doc = "# One
#notaheading
####### seven
```
# in fence
```
~~~
## in tilde
~~~
## Two ##
    # indented code
###### Six
#
";
        let (found, capped) = headings_of(doc, u64::MAX, 500);
        assert!(!capped);
        assert_eq!(pairs(&found), vec![(1, "One"), (2, "Two"), (6, "Six")]);
    }

    #[test]
    fn heading_text_is_trimmed_and_closing_hashes_stripped() {
        let (found, _) = headings_of(
            "##   Spaced   
# C#
### Title ###   
# ###
",
            u64::MAX,
            500,
        );
        assert_eq!(pairs(&found), vec![(2, "Spaced"), (1, "C#"), (3, "Title")]);
    }

    #[test]
    fn a_longer_fence_is_not_closed_by_a_shorter_one() {
        let doc = "````
```
# still code
````
# real
";
        let (found, _) = headings_of(doc, u64::MAX, 500);
        assert_eq!(pairs(&found), vec![(1, "real")]);
    }

    #[test]
    fn heading_cap_reports_truncation() {
        let doc = "# a
# b
# c
";
        let (found, capped) = headings_of(doc, u64::MAX, 2);
        assert_eq!(found.len(), 2);
        assert!(capped);
        let (found, capped) = headings_of(doc, u64::MAX, 3);
        assert_eq!(found.len(), 3);
        assert!(!capped);
    }

    #[test]
    fn in_preview_is_true_only_for_lines_inside_the_cut() {
        let doc = "# a



# b
";
        // "# a" ends at byte 3, "# b" at byte 12
        let (found, _) = headings_of(doc, 3, 500);
        assert_eq!(
            found.iter().map(|h| h.in_preview).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn read_limited_returns_whole_small_files_with_outline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(
            &path,
            "# Hi

text
",
        )
        .unwrap();
        let out = read_limited(&path, 1024).unwrap();
        assert_eq!(
            out.text,
            "# Hi

text
"
        );
        assert!(!out.truncated);
        assert_eq!(out.total_bytes, 11);
        assert_eq!(pairs(&out.headings), vec![(1, "Hi")]);
        assert!(out.headings[0].in_preview);
        assert!(!out.headings_truncated);
    }

    #[test]
    fn read_limited_cuts_large_files_at_a_paragraph_and_outlines_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let doc = "# First

alpha beta

## Second

gamma delta epsilon zeta

## Late

end
";
        std::fs::write(&path, doc).unwrap();
        let out = read_limited(&path, 40).unwrap();
        assert!(out.truncated);
        assert_eq!(out.total_bytes, doc.len() as u64);
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

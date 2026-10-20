//! ocrs model files in `<models root>/ocrs/`: pinned URLs and checksums, download with progress
//! and cancellation, verification and atomic installation. Only `fetch`/`fetch_ocrs` use the
//! network, and they run only when the user asks for a download.

use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cancel::CancelToken;

/// Sub-folder of the models root that holds the ocrs files.
pub const OCRS_DIR: &str = "ocrs";
pub const MODELS_ENV: &str = "OMNIMARK_MODELS_DIR";
/// Same as the Tauri identifier, so the CLI and the app share one models folder.
pub const APP_IDENTIFIER: &str = "io.github.keremv1.omnimark";
pub const DETECTION_MODEL: &str = "text-detection.onnx";
pub const RECOGNITION_MODEL: &str = "text-recognition.onnx";
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    /// Lower-case hex SHA-256.
    pub sha256: &'static str,
    pub size: u64,
}

/// Downloaded and hashed on 2026-10-19 (ocrs 0.13.1, models dated 2026-09-13).
pub const OCRS_MODELS: [ModelFile; 2] = [
    ModelFile {
        name: DETECTION_MODEL,
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx",
        sha256: "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe",
        size: 2_499_479,
    },
    ModelFile {
        name: RECOGNITION_MODEL,
        url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.onnx",
        sha256: "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5",
        size: 9_713_177,
    },
];

pub fn ocrs_dir(models_root: &Path) -> PathBuf {
    models_root.join(OCRS_DIR)
}

/// `<local data>/io.github.keremv1.omnimark/models` (Windows: `%LOCALAPPDATA%\…`).
pub fn default_models_root() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join(APP_IDENTIFIER).join("models"))
}

/// `--models-dir` > `OMNIMARK_MODELS_DIR` (non-empty) > the platform default.
pub fn resolve_models_root(
    explicit: Option<PathBuf>,
    env: Option<OsString>,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit
        .or_else(|| env.filter(|value| !value.is_empty()).map(PathBuf::from))
        .or(default)
}

fn has_size(path: &Path, size: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == size)
}

/// True when every ocrs model file is present with its expected size (no hashing).
pub fn ocrs_models_present(models_root: &Path) -> bool {
    let dir = ocrs_dir(models_root);
    OCRS_MODELS
        .iter()
        .all(|file| has_size(&dir.join(file.name), file.size))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub name: String,
    pub size: u64,
    pub present: bool,
}

pub fn ocrs_status(models_root: &Path) -> Vec<ModelStatus> {
    let dir = ocrs_dir(models_root);
    OCRS_MODELS
        .iter()
        .map(|file| ModelStatus {
            name: file.name.to_string(),
            size: file.size,
            present: has_size(&dir.join(file.name), file.size),
        })
        .collect()
}

pub fn ocrs_download_size() -> u64 {
    OCRS_MODELS.iter().map(|file| file.size).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download failed: {0}")]
    Network(String),
    #[error("{file} has an unexpected size ({actual} bytes)")]
    Size { file: String, actual: u64 },
    #[error("{file} failed the SHA-256 check")]
    Checksum { file: String },
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("download cancelled")]
    Cancelled,
}

pub fn fetch_ocrs(
    models_root: &Path,
    cancel: &CancelToken,
    progress: &dyn Fn(DownloadProgress),
) -> Result<(), DownloadError> {
    fetch(&ocrs_dir(models_root), &OCRS_MODELS, cancel, progress)
}

/// Downloads every file of `files` that is not already present and verified.
pub fn fetch(
    dir: &Path,
    files: &[ModelFile],
    cancel: &CancelToken,
    progress: &dyn Fn(DownloadProgress),
) -> Result<(), DownloadError> {
    fetch_with(dir, files, cancel, progress, Timeouts::default())
}

/// Network timeouts of one download. A stalled server must fail instead of hanging forever.
#[derive(Clone, Copy)]
struct Timeouts {
    connect: Duration,
    recv_response: Duration,
    recv_body: Duration,
    global: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(20),
            recv_response: Duration::from_secs(30),
            // ureq has no idle-read timeout; this caps one model file (~10 MB) even on a slow link.
            recv_body: Duration::from_secs(10 * 60),
            global: Duration::from_secs(15 * 60),
        }
    }
}

fn fetch_with(
    dir: &Path,
    files: &[ModelFile],
    cancel: &CancelToken,
    progress: &dyn Fn(DownloadProgress),
    timeouts: Timeouts,
) -> Result<(), DownloadError> {
    std::fs::create_dir_all(dir)?;
    remove_stale_parts(dir);
    let total: u64 = files.iter().map(|file| file.size).sum();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(timeouts.connect))
        .timeout_recv_response(Some(timeouts.recv_response))
        .timeout_recv_body(Some(timeouts.recv_body))
        .timeout_global(Some(timeouts.global))
        .build()
        .into();
    let mut done = 0u64;
    for file in files {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        if verify(&dir.join(file.name), file)? {
            done += file.size;
            progress(DownloadProgress {
                downloaded: done,
                total,
            });
            continue;
        }
        let response = agent
            .get(file.url)
            .call()
            .map_err(|e| DownloadError::Network(format!("{}: {e}", file.url)))?;
        let mut reader = response
            .into_body()
            .into_with_config()
            .limit(file.size + 1)
            .reader();
        let base = done;
        install(&mut reader, dir, file, cancel, &mut |written| {
            progress(DownloadProgress {
                downloaded: base + written,
                total,
            })
        })?;
        done += file.size;
    }
    progress(DownloadProgress {
        downloaded: total,
        total,
    });
    Ok(())
}

/// Size and SHA-256 of an existing file; `Ok(false)` when it is missing or different.
pub fn verify(path: &Path, file: &ModelFile) -> std::io::Result<bool> {
    if !has_size(path, file.size) {
        return Ok(false);
    }
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex(&hasher.finalize()) == file.sha256)
}

/// Best effort: removes `*.part` files left behind by crashed downloads (older than an hour).
fn remove_stale_parts(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = path.extension().is_some_and(|e| e == "part")
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(3600));
        if stale {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Streams `reader` into a unique `<dir>/<name>.<pid>-<nanos>.part`, checks it, then renames it into place. The
/// partial file is removed on any failure, including cancellation.
pub(crate) fn install(
    reader: &mut dyn Read,
    dir: &Path,
    file: &ModelFile,
    cancel: &CancelToken,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let part = dir.join(format!("{}.{}-{nanos}.part", file.name, std::process::id()));
    let result = write_checked(reader, &part, file, cancel, on_bytes)
        .and_then(|()| std::fs::rename(&part, dir.join(file.name)).map_err(DownloadError::from));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn write_checked(
    reader: &mut dyn Read,
    part: &Path,
    file: &ModelFile,
    cancel: &CancelToken,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(part)?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(DownloadError::Network(e.to_string())),
        };
        written += n as u64;
        if written > file.size {
            return Err(DownloadError::Size {
                file: file.name.to_string(),
                actual: written,
            });
        }
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
        on_bytes(written);
    }
    if written != file.size {
        return Err(DownloadError::Size {
            file: file.name.to_string(),
            actual: written,
        });
    }
    if hex(&hasher.finalize()) != file.sha256 {
        return Err(DownloadError::Checksum {
            file: file.name.to_string(),
        });
    }
    out.sync_all()?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn model(name: &'static str, url: &'static str, data: &[u8]) -> ModelFile {
        ModelFile {
            name,
            url,
            sha256: leak(hex(&Sha256::digest(data))),
            size: data.len() as u64,
        }
    }

    /// Serves `bodies` to consecutive connections over plain HTTP.
    fn serve(bodies: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (format!("http://{address}"), handle)
    }

    #[test]
    fn pinned_models_are_https_with_full_checksums() {
        assert_eq!(ocrs_download_size(), 12_212_656);
        for file in &OCRS_MODELS {
            assert!(file
                .url
                .starts_with("https://ocrs-models.s3-accelerate.amazonaws.com/"));
            assert!(file.url.ends_with(file.name));
            assert_eq!(file.sha256.len(), 64);
        }
    }

    #[test]
    fn models_root_resolution_order() {
        let flag = Some(PathBuf::from("flag"));
        let env = Some(OsString::from("env"));
        let default = Some(PathBuf::from("default"));
        assert_eq!(
            resolve_models_root(flag.clone(), env.clone(), default.clone()),
            flag
        );
        assert_eq!(
            resolve_models_root(None, env, default.clone()),
            Some(PathBuf::from("env"))
        );
        assert_eq!(
            resolve_models_root(None, Some(OsString::new()), default.clone()),
            default
        );
        assert!(
            default_models_root().is_some_and(|p| p.ends_with("io.github.keremv1.omnimark/models"))
        );
    }

    #[test]
    fn install_is_atomic_and_rejects_bad_checksums_sizes_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let data = b"model bytes".as_slice();
        let good = model("m.onnx", "unused", data);
        let cancel = CancelToken::new();
        install(&mut &data[..], dir.path(), &good, &cancel, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(dir.path().join("m.onnx")).unwrap(), data);
        assert!(verify(&dir.path().join("m.onnx"), &good).unwrap());

        let bad_sum = ModelFile {
            name: "b.onnx",
            sha256: leak("0".repeat(64)),
            ..good
        };
        assert!(matches!(
            install(&mut &data[..], dir.path(), &bad_sum, &cancel, &mut |_| {}),
            Err(DownloadError::Checksum { .. })
        ));
        let too_long = ModelFile {
            name: "c.onnx",
            size: 3,
            ..good
        };
        assert!(matches!(
            install(&mut &data[..], dir.path(), &too_long, &cancel, &mut |_| {}),
            Err(DownloadError::Size { .. })
        ));
        let cancelled = CancelToken::new();
        cancelled.cancel();
        let d = ModelFile {
            name: "d.onnx",
            ..good
        };
        assert!(matches!(
            install(&mut &data[..], dir.path(), &d, &cancelled, &mut |_| {}),
            Err(DownloadError::Cancelled)
        ));
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["m.onnx"], "no partial or failed files may remain");
    }

    #[test]
    fn fetch_downloads_with_progress_then_skips_verified_files() {
        let (a, b) = (vec![1u8; 70_000], vec![2u8; 10]);
        let (base, server) = serve(vec![a.clone(), b.clone()]);
        let files = [
            model("a.onnx", leak(format!("{base}/a.onnx")), &a),
            model("b.onnx", leak(format!("{base}/b.onnx")), &b),
        ];
        let dir = tempfile::tempdir().unwrap();
        let seen = Mutex::new(Vec::new());
        fetch(dir.path(), &files, &CancelToken::new(), &|p| {
            seen.lock().unwrap().push(p)
        })
        .unwrap();
        server.join().unwrap();
        let seen = seen.into_inner().unwrap();
        assert!(seen.windows(2).all(|w| w[0].downloaded <= w[1].downloaded));
        assert_eq!(
            seen.last(),
            Some(&DownloadProgress {
                downloaded: 70_010,
                total: 70_010
            })
        );
        // The server is gone: a second fetch must not connect at all.
        fetch(dir.path(), &files, &CancelToken::new(), &|_| {}).unwrap();
    }

    /// Serves a 200 response whose body is cut short or never arrives.
    fn serve_raw(responses: Vec<Vec<u8>>, stall: bool) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                stream.write_all(&response).unwrap();
                if stall {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                }
            }
        });
        (format!("http://{address}"), handle)
    }

    fn part_files(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part"))
            .collect()
    }

    #[test]
    fn stalled_download_times_out_and_leaves_no_part_file() {
        let head =
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nabc".to_vec();
        let (base, server) = serve_raw(vec![head], true);
        let data = vec![7u8; 100];
        let files = [model("s.onnx", leak(format!("{base}/s.onnx")), &data)];
        let dir = tempfile::tempdir().unwrap();
        let timeouts = Timeouts {
            connect: std::time::Duration::from_secs(1),
            recv_response: std::time::Duration::from_secs(1),
            recv_body: std::time::Duration::from_secs(1),
            global: std::time::Duration::from_secs(2),
        };
        let started = std::time::Instant::now();
        let result = fetch_with(dir.path(), &files, &CancelToken::new(), &|_| {}, timeouts);
        assert!(
            matches!(result, Err(DownloadError::Network(_))),
            "{result:?}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
        assert!(part_files(dir.path()).is_empty());
        server.join().unwrap();
    }

    #[test]
    fn short_body_is_rejected_without_leftovers() {
        let data = vec![5u8; 100];
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789"
            .to_vec();
        let (base, server) = serve_raw(vec![head], false);
        let files = [model("t.onnx", leak(format!("{base}/t.onnx")), &data)];
        let dir = tempfile::tempdir().unwrap();
        let result = fetch(dir.path(), &files, &CancelToken::new(), &|_| {});
        assert!(
            matches!(result, Err(DownloadError::Size { .. })),
            "{result:?}"
        );
        assert!(part_files(dir.path()).is_empty());
        assert!(!dir.path().join("t.onnx").exists());
        server.join().unwrap();
    }

    #[test]
    fn http_error_for_second_file_keeps_the_first_and_leaves_no_part_file() {
        let a = vec![1u8; 20];
        let ok = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            a.len()
        );
        let mut first = ok.into_bytes();
        first.extend_from_slice(&a);
        let second =
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        let (base, server) = serve_raw(vec![first, second], false);
        let b = vec![2u8; 20];
        let files = [
            model("a.onnx", leak(format!("{base}/a.onnx")), &a),
            model("b.onnx", leak(format!("{base}/b.onnx")), &b),
        ];
        let dir = tempfile::tempdir().unwrap();
        let result = fetch(dir.path(), &files, &CancelToken::new(), &|_| {});
        assert!(
            matches!(result, Err(DownloadError::Network(_))),
            "{result:?}"
        );
        assert!(verify(&dir.path().join("a.onnx"), &files[0]).unwrap());
        assert!(!dir.path().join("b.onnx").exists());
        assert!(part_files(dir.path()).is_empty());
        server.join().unwrap();
    }

    #[test]
    fn presence_is_a_size_check_over_both_ocrs_files() {
        let root = tempfile::tempdir().unwrap();
        assert!(!ocrs_models_present(root.path()));
        std::fs::create_dir_all(ocrs_dir(root.path())).unwrap();
        for file in &OCRS_MODELS {
            std::fs::File::create(ocrs_dir(root.path()).join(file.name))
                .unwrap()
                .set_len(file.size)
                .unwrap();
        }
        assert!(ocrs_models_present(root.path()));
        assert!(ocrs_status(root.path()).iter().all(|s| s.present));
    }
}

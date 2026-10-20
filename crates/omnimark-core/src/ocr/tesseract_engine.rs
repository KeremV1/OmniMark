//! Tesseract engine: runs the `tesseract` program installed on the system. OmniMark never
//! downloads or bundles it. No shell, a fixed argument list and a timeout on every call.

use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, RgbaImage};

use super::{OcrEngine, OcrError, OcrImage, OcrLanguage};

pub const TESSERACT_ENV: &str = "OMNIMARK_TESSERACT";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;
const RECOGNIZE_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    }
}

fn well_known() -> Vec<PathBuf> {
    if cfg!(windows) {
        let under = |var: &str, rest: &[&str]| {
            std::env::var_os(var).map(|base| {
                rest.iter()
                    .fold(PathBuf::from(base), |p, part| p.join(part))
            })
        };
        [
            under("ProgramFiles", &["Tesseract-OCR", "tesseract.exe"]),
            under(
                "LOCALAPPDATA",
                &["Programs", "Tesseract-OCR", "tesseract.exe"],
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else if cfg!(target_os = "macos") {
        vec![
            "/opt/homebrew/bin/tesseract".into(),
            "/usr/local/bin/tesseract".into(),
        ]
    } else {
        vec![
            "/usr/bin/tesseract".into(),
            "/usr/local/bin/tesseract".into(),
        ]
    }
}

/// Search order: configured → `OMNIMARK_TESSERACT` → `PATH` → well-known install locations.
pub fn candidate_paths(
    configured: Option<&Path>,
    env: Option<OsString>,
    path_var: Option<OsString>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = configured.map(Path::to_path_buf).into_iter().collect();
    out.extend(env.filter(|value| !value.is_empty()).map(PathBuf::from));
    if let Some(path_var) = path_var {
        out.extend(
            std::env::split_paths(&path_var)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(exe_name())),
        );
    }
    out.extend(well_known());
    out
}

/// Only an existing file whose name (without extension) is `tesseract`, in any case.
pub fn is_acceptable(path: &Path) -> bool {
    let exe_ok = !cfg!(windows)
        || path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    exe_ok
        && path.is_file()
        && path
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| stem.eq_ignore_ascii_case("tesseract"))
}

/// The first acceptable candidate whose `--version` succeeds within 5 s.
pub fn resolve_path(configured: Option<&Path>) -> Option<PathBuf> {
    candidate_paths(
        configured,
        std::env::var_os(TESSERACT_ENV),
        std::env::var_os("PATH"),
    )
    .into_iter()
    .find(|path| {
        is_acceptable(path)
            && run(path, &[OsStr::new("--version")], PROBE_TIMEOUT).is_ok_and(|out| out.success)
    })
}

/// Installed language codes (`--list-langs`), without `osd`; empty on any failure.
pub fn languages(path: &Path) -> Vec<String> {
    match run(path, &[OsStr::new("--list-langs")], PROBE_TIMEOUT) {
        Ok(out) => {
            let from_stdout = parse_list_langs(&out.stdout);
            if from_stdout.is_empty() {
                parse_list_langs(&out.stderr)
            } else {
                from_stdout
            }
        }
        Err(_) => Vec::new(),
    }
}

pub fn parse_list_langs(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("List of available languages") && *line != "osd"
        })
        .filter(|line| {
            line.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/'))
        })
        .map(String::from)
        .collect()
}

pub fn lang_arg(requested: OcrLanguage, installed: &[String]) -> Result<String, OcrError> {
    let has = |code: &str| installed.iter().any(|lang| lang == code);
    let wanted = match requested {
        OcrLanguage::Tr => Some("tur"),
        OcrLanguage::En => Some("eng"),
        OcrLanguage::Auto => None,
    };
    if let Some(code) = wanted {
        return if has(code) {
            Ok(code.to_string())
        } else {
            Err(OcrError::Unavailable(format!(
                "Tesseract language data '{code}' is not installed"
            )))
        };
    }
    let preferred: Vec<&str> = ["tur", "eng"]
        .into_iter()
        .filter(|code| has(code))
        .collect();
    if !preferred.is_empty() {
        return Ok(preferred.join("+"));
    }
    installed
        .first()
        .cloned()
        .ok_or_else(|| OcrError::Unavailable("Tesseract has no language data".into()))
}

struct Output {
    success: bool,
    stdout: String,
    stderr: String,
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = (&mut pipe).take(MAX_OUTPUT).read_to_end(&mut buffer);
            // Keep draining so the child never blocks on a full pipe.
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        }
        buffer
    })
}

fn collect(handle: JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&handle.join().unwrap_or_default()).into_owned()
}

/// Runs `program args…` without a shell; kills it after `timeout`.
fn run(program: &Path, args: &[&OsStr], timeout: Duration) -> Result<Output, OcrError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| OcrError::Unavailable(format!("{}: {e}", program.display())))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OcrError::Engine(format!(
                    "tesseract did not finish within {} s",
                    timeout.as_secs()
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OcrError::Engine(e.to_string()));
            }
        }
    };
    Ok(Output {
        success: status.success(),
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

/// A PNG in the temp folder that is deleted on drop (core has `tempfile` only for tests).
struct TempPng(PathBuf);

impl TempPng {
    fn write(image: &OcrImage) -> Result<Self, OcrError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let name = format!(
            "omnimark-ocr-{}-{}-{nanos}.png",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        let pixels = RgbaImage::from_raw(image.width(), image.height(), image.rgba().to_vec())
            .ok_or_else(|| OcrError::Image("invalid RGBA buffer".into()))?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| OcrError::Image(e.to_string()))?;
        let guard = Self(path);
        let mut writer = BufWriter::new(file);
        DynamicImage::ImageRgba8(pixels)
            .write_to(&mut writer, ImageFormat::Png)
            .map_err(|e| OcrError::Image(e.to_string()))?;
        writer.flush().map_err(|e| OcrError::Image(e.to_string()))?;
        Ok(guard)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempPng {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub struct TesseractEngine {
    path: PathBuf,
    languages: Vec<String>,
}

impl TesseractEngine {
    pub fn new(path: PathBuf) -> Result<Self, OcrError> {
        if !is_acceptable(&path) {
            return Err(OcrError::Unavailable(format!(
                "{} is not a tesseract program",
                path.display()
            )));
        }
        let languages = languages(&path);
        if languages.is_empty() {
            return Err(OcrError::Unavailable(
                "Tesseract has no language data".into(),
            ));
        }
        Ok(Self { path, languages })
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }
}

impl OcrEngine for TesseractEngine {
    fn name(&self) -> &'static str {
        "tesseract"
    }

    fn recognize(&self, image: &OcrImage, language: &OcrLanguage) -> Result<String, OcrError> {
        let langs = lang_arg(*language, &self.languages)?;
        let png = TempPng::write(image)?;
        let args = [
            png.path().as_os_str(),
            OsStr::new("stdout"),
            OsStr::new("-l"),
            OsStr::new(&langs),
            OsStr::new("--psm"),
            OsStr::new("3"),
        ];
        let out = run(&self.path, &args, RECOGNIZE_TIMEOUT)?;
        if !out.success {
            let first = out
                .stderr
                .lines()
                .next()
                .unwrap_or("tesseract failed")
                .trim();
            return Err(OcrError::Engine(first.to_string()));
        }
        Ok(out.stdout.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::test_render::{render_text, system_font};

    fn installed(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn candidates_follow_configured_env_path_then_well_known() {
        let base = std::env::temp_dir();
        let (dir1, dir2) = (base.join("dir1"), base.join("dir2"));
        let path_var = std::env::join_paths([dir1.clone(), dir2.clone()]).unwrap();
        let list = candidate_paths(
            Some(Path::new("conf/tesseract")),
            Some(OsString::from("env/tesseract")),
            Some(path_var),
        );
        let exe = if cfg!(windows) {
            "tesseract.exe"
        } else {
            "tesseract"
        };
        assert_eq!(list[0], PathBuf::from("conf/tesseract"));
        assert_eq!(list[1], PathBuf::from("env/tesseract"));
        assert_eq!(list[2], dir1.join(exe));
        assert_eq!(list[3], dir2.join(exe));
        assert!(list.len() > 4, "well-known locations follow");
        let without_env = candidate_paths(None, Some(OsString::new()), None);
        assert!(without_env
            .iter()
            .all(|p| p.file_stem().unwrap().eq_ignore_ascii_case("tesseract")));
    }

    #[test]
    fn relative_and_empty_path_entries_are_skipped() {
        let abs = std::env::temp_dir().join("abs-dir");
        let path_var =
            std::env::join_paths([PathBuf::from("relative"), PathBuf::new(), abs.clone()]).unwrap();
        let exe = if cfg!(windows) {
            "tesseract.exe"
        } else {
            "tesseract"
        };
        let list = candidate_paths(None, None, Some(path_var));
        assert_eq!(list[0], abs.join(exe));
        assert!(!list.contains(&Path::new("relative").join(exe)));
        assert!(!list.contains(&PathBuf::from(exe)));
    }

    #[test]
    fn only_existing_files_named_tesseract_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "tesseract",
            "Tesseract.EXE",
            "evil.exe",
            "tesseract-wrapper",
            "tesseract.bat",
            "tesseract.cmd",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::create_dir(dir.path().join("sub/tesseract.exe")).unwrap();
        // Windows runs only real executables, so a bare name is refused there.
        assert_eq!(is_acceptable(&dir.path().join("tesseract")), !cfg!(windows));
        assert_eq!(
            is_acceptable(&dir.path().join("tesseract.bat")),
            !cfg!(windows)
        );
        assert_eq!(
            is_acceptable(&dir.path().join("tesseract.cmd")),
            !cfg!(windows)
        );
        assert!(is_acceptable(&dir.path().join("Tesseract.EXE")));
        assert!(!is_acceptable(&dir.path().join("evil.exe")));
        assert!(!is_acceptable(&dir.path().join("tesseract-wrapper")));
        assert!(
            !is_acceptable(&dir.path().join("sub/tesseract.exe")),
            "directories are refused"
        );
        assert!(!is_acceptable(&dir.path().join("missing/tesseract")));
    }

    #[test]
    fn list_langs_output_is_parsed() {
        let out = "List of available languages in \"/usr/share/tesseract-ocr/5/tessdata/\" (4):\neng\nosd\ntur\nscript/Latin\nbad; rm -rf\n";
        assert_eq!(parse_list_langs(out), ["eng", "tur", "script/Latin"]);
        assert!(parse_list_langs("").is_empty());
    }

    #[test]
    fn language_arguments() {
        let both = installed(&["eng", "tur", "deu"]);
        assert_eq!(lang_arg(OcrLanguage::Tr, &both).unwrap(), "tur");
        assert_eq!(lang_arg(OcrLanguage::En, &both).unwrap(), "eng");
        assert_eq!(lang_arg(OcrLanguage::Auto, &both).unwrap(), "tur+eng");
        assert_eq!(
            lang_arg(OcrLanguage::Auto, &installed(&["eng"])).unwrap(),
            "eng"
        );
        assert_eq!(
            lang_arg(OcrLanguage::Auto, &installed(&["deu", "fra"])).unwrap(),
            "deu"
        );
        assert_eq!(
            lang_arg(OcrLanguage::Tr, &installed(&["eng"])).unwrap_err(),
            OcrError::Unavailable("Tesseract language data 'tur' is not installed".into())
        );
        assert!(lang_arg(OcrLanguage::Auto, &[]).is_err());
    }

    #[test]
    fn temporary_png_is_removed_when_dropped() {
        let image = OcrImage::new(20, 20, vec![255; 20 * 20 * 4]).unwrap();
        let png = TempPng::write(&image).unwrap();
        let path = png.path().to_path_buf();
        assert!(path.is_file());
        assert!(image::open(&path).is_ok());
        drop(png);
        assert!(!path.exists());
    }

    #[test]
    fn engine_rejects_programs_with_other_names() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("evil.exe");
        std::fs::write(&other, b"x").unwrap();
        assert!(matches!(
            TesseractEngine::new(other),
            Err(OcrError::Unavailable(_))
        ));
    }

    /// Runs only when Tesseract is installed (CI installs it on Linux and macOS).
    #[test]
    fn reads_rendered_text_with_the_installed_tesseract() {
        let vendored = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../vendor/tesseract/tesseract.exe"
        ));
        let Some(path) = std::env::var_os(TESSERACT_ENV)
            .map(PathBuf::from)
            .filter(|p| is_acceptable(p))
            .or_else(|| (cfg!(windows) && is_acceptable(vendored)).then(|| vendored.to_path_buf()))
            .or_else(|| resolve_path(None))
        else {
            eprintln!("tesseract not installed; skipping");
            return;
        };
        let Some(font) = system_font() else {
            eprintln!("no system font; skipping");
            return;
        };
        let engine = TesseractEngine::new(path).unwrap();
        assert_eq!(engine.name(), "tesseract");
        let english = render_text("The quick brown fox jumps over 12 lazy dogs.", &font, 32.0);
        let language = if engine.languages().iter().any(|l| l == "eng") {
            OcrLanguage::En
        } else {
            OcrLanguage::Auto
        };
        let text = engine.recognize(&english, &language).unwrap();
        eprintln!("tesseract en: {text:?}");
        assert!(text.contains("quick brown"), "{text:?}");
        if engine.languages().iter().any(|l| l == "tur") {
            let turkish = render_text("Merhaba dünya, bugün hava güzel.", &font, 32.0);
            let text = engine.recognize(&turkish, &OcrLanguage::Tr).unwrap();
            eprintln!("tesseract tr: {text:?}");
            assert!(text.contains("dünya"), "{text:?}");
        }
    }
}

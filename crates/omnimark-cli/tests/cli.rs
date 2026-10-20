use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/markitdown/files")
        .join(name)
}

#[test]
fn unsupported_file_fails_with_exit_code_1_and_writes_nothing() {
    let out = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["convert", "-o"])
        .arg(out.path())
        .arg(fixture("random.bin"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported format"));
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

#[test]
fn help_lists_the_convert_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("convert"));
}

#[test]
fn converts_a_text_file_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("notes.txt");
    std::fs::write(
        &input, "merhaba
",
    )
    .unwrap();
    let out = dir.path().join("out");
    for _ in 0..2 {
        let status = Command::new(env!("CARGO_BIN_EXE_omnimark"))
            .args(["convert", "-o"])
            .arg(&out)
            .arg(&input)
            .status()
            .unwrap();
        assert!(status.success());
    }
    assert_eq!(
        std::fs::read_to_string(out.join("notes.md")).unwrap(),
        "merhaba"
    );
    assert_eq!(
        std::fs::read_to_string(out.join("notes (2).md")).unwrap(),
        "merhaba"
    );
}

#[test]
fn pdf_uses_vendor_pdfium_from_the_current_dir_by_default() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let vendored = root
        .join("vendor/pdfium")
        .join(omnimark_core::formats::pdfium_library_file_name());
    if !vendored.is_file() {
        eprintln!("skipped: run scripts/fetch-pdfium first");
        return;
    }
    let out = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .current_dir(&root)
        .env_remove("OMNIMARK_PDFIUM_PATH")
        .args(["convert", "--stdout"])
        .arg(fixture("test.pdf"))
        .arg("-o")
        .arg(out.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty());
}

#[test]
fn models_list_reports_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["models", "list", "--models-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("text-detection.onnx") && stdout.contains("text-recognition.onnx"));
    assert_eq!(stdout.matches("missing").count(), 2, "{stdout}");
    assert!(!stdout.contains("tesseract"), "{stdout}");
}

#[test]
fn ocr_without_downloaded_models_fails_before_converting() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a.txt");
    std::fs::write(&input, "x").unwrap();
    let out = dir.path().join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["convert", "--ocr=ocrs", "--models-dir"])
        .arg(dir.path().join("models"))
        .arg("-o")
        .arg(&out)
        .arg(&input)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ocrs models are not downloaded"));
    assert!(!out.exists());
}

#[test]
fn ocr_values_are_validated_and_documented() {
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["convert", "--ocr=bogus", "a.txt"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let help = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["convert", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--ocr") && help.contains("--ocr-lang") && help.contains("--models-dir"));
    assert!(!help.contains("tesseract"));
}

#[test]
fn stdout_mode_prints_warnings_to_stderr() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let vendored = root
        .join("vendor/pdfium")
        .join(omnimark_core::formats::pdfium_library_file_name());
    let scanned = root.join("tests/fixtures/markitdown/ocr/pdf_scanned_minimal.pdf");
    if !vendored.is_file() || !scanned.is_file() {
        eprintln!("skipped: pdfium or the scanned fixture is missing");
        return;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .env("OMNIMARK_PDFIUM_PATH", &vendored)
        .args(["convert", "--stdout"])
        .arg(&scanned)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning ") && stderr.contains("pdf_scanned_minimal.pdf"),
        "{stderr}"
    );
}

fn make_zip(path: &Path) {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in [
        ("a.txt", &b"alpha"[..]),
        ("dir/b.txt", b"beta"),
        ("c.bin", &[0u8, 1]),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn zip_defaults_to_one_combined_file_and_supports_folder_and_zip_modes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pack.zip");
    make_zip(&input);
    let run = |extra: &[&str], out: &Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_omnimark"))
            .args(["convert", "-o"])
            .arg(out)
            .args(extra)
            .arg(&input)
            .output()
            .unwrap();
        assert!(output.status.success(), "{extra:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let single = dir.path().join("single");
    let stdout = run(&[], &single);
    assert!(stdout.contains("2 files converted, 1 skipped"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(single.join("pack.md")).unwrap(),
        "Content from the zip file `pack.zip`:

## File: a.txt

alpha

## File: dir/b.txt

beta"
    );

    let folder = dir.path().join("folder");
    run(&["--zip-output", "folder"], &folder);
    assert_eq!(
        std::fs::read_to_string(folder.join("pack/dir/b.md")).unwrap(),
        "beta"
    );

    let packed = dir.path().join("packed");
    run(&["--zip-output=zip"], &packed);
    assert!(packed.join("pack-md.zip").is_file());

    // --stdout prints the combined text, like MarkItDown.
    let printed = Command::new(env!("CARGO_BIN_EXE_omnimark"))
        .args(["convert", "--stdout"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&printed.stdout)
        .starts_with("Content from the zip file `pack.zip`:"));
}

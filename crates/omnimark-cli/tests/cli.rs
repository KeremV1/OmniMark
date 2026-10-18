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

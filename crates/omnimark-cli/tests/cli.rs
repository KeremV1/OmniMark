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

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use dup_remover::ScanReport;
use tempfile::{TempDir, tempdir};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dup-remover"))
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", stderr(output));
}

fn duplicate_tree() -> TempDir {
    let directory = tempdir().unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();
    for name in ["original.txt", "copy.txt", "nested/third.txt"] {
        fs::write(directory.path().join(name), b"rust").unwrap();
    }
    fs::write(directory.path().join("other.txt"), b"code").unwrap();
    fs::write(directory.path().join("unique.txt"), b"unique").unwrap();
    directory
}

fn scan(directory: &Path) -> Command {
    let mut command = binary();
    command.arg("scan").arg(directory);
    command
}

#[test]
fn help_and_version_are_available_without_scanning() {
    let help = binary().arg("--help").output().unwrap();
    assert_success(&help);
    assert!(stdout(&help).contains("scan"));

    let scan_help = binary().args(["scan", "--help"]).output().unwrap();
    assert_success(&scan_help);
    for flag in ["<PATH>", "--min-size", "--jobs", "--output"] {
        assert!(stdout(&scan_help).contains(flag), "missing {flag}");
    }

    for args in [vec!["--version"], vec!["scan", "--version"]] {
        let version = binary().args(args).output().unwrap();
        assert_success(&version);
        assert!(stdout(&version).contains(env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn invalid_arguments_fail_before_scanning() {
    for args in [
        vec![],
        vec!["scan"],
        vec!["unknown-command"],
        vec!["scan", ".", "--min-size", "1.5MB"],
        vec!["scan", ".", "--min-size", "18446744073709551615GB"],
        vec!["scan", ".", "--jobs", "0"],
        vec!["scan", ".", "--jobs", "-1"],
        vec!["scan", ".", "--unexpected"],
    ] {
        let output = binary().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        assert!(!stderr(&output).is_empty(), "args: {args:?}");
    }
}

#[test]
fn finds_real_duplicates_and_preserves_input_files() {
    let directory = duplicate_tree();
    let output = scan(directory.path())
        .args(["--jobs", "2"])
        .output()
        .unwrap();
    assert_success(&output);
    let text = stdout(&output);
    for expected in [
        "Просканировано файлов: 5",
        "Общий размер: 22 байт",
        "Групп дубликатов: 1",
        "Лишних копий: 2",
        "Объём лишних копий: 8 байт",
        "original.txt",
        "copy.txt",
        "third.txt",
    ] {
        assert!(text.contains(expected), "missing {expected:?}: {text}");
    }
    assert_eq!(
        fs::read(directory.path().join("original.txt")).unwrap(),
        b"rust"
    );
    assert_eq!(
        fs::read(directory.path().join("copy.txt")).unwrap(),
        b"rust"
    );
    assert_eq!(
        fs::read(directory.path().join("nested/third.txt")).unwrap(),
        b"rust"
    );
}

#[test]
fn saves_the_full_report_to_json() {
    let directory = duplicate_tree();
    let output_directory = tempdir().unwrap();
    let path = output_directory.path().join("report.json");
    let output = scan(directory.path())
        .args(["--jobs", "1"])
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert_success(&output);
    let report: ScanReport = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report.scanned_files, 5);
    assert_eq!(report.scanned_bytes, 22);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.duplicate_files(), 2);
    assert_eq!(report.reclaimable_bytes(), 8);
    assert!(report.warnings.is_empty());
    let group = &report.duplicate_groups[0];
    assert_eq!(group.size_bytes, 4);
    assert_eq!(group.hash, blake3::hash(b"rust").to_hex().to_string());
    let mut paths: Vec<PathBuf> = ["original.txt", "copy.txt", "nested/third.txt"]
        .iter()
        .map(|name| directory.path().join(name))
        .collect();
    paths.sort();
    assert_eq!(group.paths, paths);
}

#[test]
fn minimum_size_filters_small_files() {
    let directory = duplicate_tree();
    let output = scan(directory.path())
        .args(["--min-size", "5B"])
        .output()
        .unwrap();
    assert_success(&output);
    let text = stdout(&output);
    assert!(text.contains("Просканировано файлов: 1"));
    assert!(text.contains("Общий размер: 6 байт"));
    assert!(text.contains("Групп дубликатов: 0"));
    assert!(text.contains("Дубликаты не найдены."));
}

#[test]
fn output_does_not_overwrite_a_scanned_file() {
    let directory = duplicate_tree();
    let original = directory.path().join("original.txt");
    let output = scan(directory.path())
        .arg("--output")
        .arg(&original)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("JSON-отчёт"));
    assert_eq!(fs::read(&original).unwrap(), b"rust");
}

#[test]
fn invalid_scan_does_not_create_an_output_file() {
    let directory = tempdir().unwrap();
    let output_path = directory.path().join("report.json");
    let output = scan(&directory.path().join("missing"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Не удалось сканировать"));
    assert!(!output_path.exists());
}

#[test]
fn unavailable_output_directory_causes_failure() {
    let directory = duplicate_tree();
    let output_path = directory.path().join("missing").join("report.json");
    let output = scan(directory.path())
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("Не удалось сохранить JSON-отчёт"));
    assert!(!output_path.exists());
}

#[test]
fn a_new_output_inside_the_scan_root_is_created_after_scanning() {
    let directory = duplicate_tree();
    let output_path = directory.path().join("report.json");
    let output = scan(directory.path())
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert_success(&output);
    let report: ScanReport = serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(report.scanned_files, 5);
    assert_eq!(report.scanned_bytes, 22);
}

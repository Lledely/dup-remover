use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use dup_remover::ScanReport;

pub fn write_text(report: &ScanReport, mut output: impl Write) -> io::Result<()> {
    writeln!(output, "Просканировано файлов: {}", report.scanned_files)?;
    writeln!(output, "Общий размер: {} байт", report.scanned_bytes)?;
    writeln!(
        output,
        "Групп дубликатов: {}",
        report.duplicate_groups.len()
    )?;
    writeln!(output, "Лишних копий: {}", report.duplicate_files())?;
    writeln!(
        output,
        "Объём лишних копий: {} байт",
        report.reclaimable_bytes()
    )?;
    writeln!(output, "Предупреждений: {}", report.warnings.len())?;

    if report.duplicate_groups.is_empty() {
        writeln!(output, "\nДубликаты не найдены.")?;
    }
    for (index, group) in report.duplicate_groups.iter().enumerate() {
        writeln!(
            output,
            "\nГруппа {}: {} байт, файлов: {} (BLAKE3: {})",
            index + 1,
            group.size_bytes,
            group.paths.len(),
            group.hash
        )?;
        for path in &group.paths {
            writeln!(output, "  {}", path.display())?;
        }
    }

    if !report.warnings.is_empty() {
        writeln!(output, "\nПредупреждения:")?;
        for warning in &report.warnings {
            writeln!(output, "  {}: {}", warning.path.display(), warning.message)?;
        }
    }
    Ok(())
}

pub fn write_json(path: &Path, report: &ScanReport) -> io::Result<()> {
    // Serialize before creating the file: a non-UTF-8 path may fail serialization.
    let mut json = serde_json::to_vec_pretty(report).map_err(io::Error::other)?;
    json.push(b'\n');
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(&json)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use dup_remover::{DuplicateGroup, ScanWarning};
    use tempfile::tempdir;

    use super::*;

    fn sample_report() -> ScanReport {
        ScanReport {
            scanned_files: 4,
            scanned_bytes: 28,
            duplicate_groups: vec![DuplicateGroup {
                size_bytes: 8,
                hash: "sample-hash".to_owned(),
                paths: vec![PathBuf::from("original.txt"), PathBuf::from("copy.txt")],
            }],
            warnings: vec![ScanWarning {
                path: PathBuf::from("unreadable.txt"),
                message: "permission denied".to_owned(),
            }],
        }
    }

    #[test]
    fn text_contains_statistics_groups_and_warning_paths() {
        let mut text = Vec::new();
        write_text(&sample_report(), &mut text).unwrap();
        let text = String::from_utf8(text).unwrap();
        for expected in [
            "Просканировано файлов: 4",
            "Общий размер: 28 байт",
            "Групп дубликатов: 1",
            "Лишних копий: 1",
            "Объём лишних копий: 8 байт",
            "Предупреждений: 1",
            "Группа 1: 8 байт, файлов: 2",
            "sample-hash",
            "original.txt",
            "copy.txt",
            "unreadable.txt: permission denied",
        ] {
            assert!(text.contains(expected), "missing {expected:?}: {text}");
        }
    }

    #[test]
    fn text_explains_when_there_are_no_duplicates() {
        let mut text = Vec::new();
        write_text(&ScanReport::default(), &mut text).unwrap();
        assert!(
            String::from_utf8(text)
                .unwrap()
                .contains("Дубликаты не найдены.")
        );
    }

    #[test]
    fn json_preserves_the_complete_report() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("report.json");
        let report = sample_report();
        write_json(&path, &report).unwrap();
        let contents = fs::read(&path).unwrap();
        assert_eq!(contents.last(), Some(&b'\n'));
        assert_eq!(
            serde_json::from_slice::<ScanReport>(&contents).unwrap(),
            report
        );
    }

    #[test]
    fn json_does_not_overwrite_an_existing_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("original.txt");
        fs::write(&path, b"keep this content").unwrap();
        let error = write_json(&path, &sample_report()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"keep this content");
    }

    #[test]
    fn json_reports_an_unavailable_output_directory() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("missing").join("report.json");
        assert!(write_json(&path, &sample_report()).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn text_propagates_write_errors() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("write failed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        assert!(write_text(&sample_report(), FailingWriter).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_does_not_create_a_json_file() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let directory = tempdir().unwrap();
        let output = directory.path().join("report.json");
        let mut report = sample_report();
        report.duplicate_groups[0].paths[0] = PathBuf::from(OsString::from_vec(vec![0xff]));

        assert!(write_json(&output, &report).is_err());
        assert!(!output.exists());
    }
}

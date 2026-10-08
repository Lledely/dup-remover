//! Console formatting and exclusive creation of complete JSON reports.

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use dup_remover::ScanReport;

/// Write the text report, optionally mention an already-saved JSON file, and flush.
///
/// The filename is printed only when supplied by the caller after a successful
/// JSON save. Write and flush failures are propagated to the CLI as output errors.
pub fn write_output(
    report: &ScanReport,
    json_path: Option<&Path>,
    mut output: impl Write,
) -> io::Result<()> {
    write_text(report, &mut output)?;
    if let Some(path) = json_path {
        writeln!(output, "JSON-отчёт сохранён: {}", path.display())?;
    }
    output.flush()
}

/// Render scan counters, duplicate paths, and warnings to a writable stream.
///
/// Output follows the report order and propagates the first write failure.
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

/// Serialize the full report and exclusively create a new JSON file.
///
/// Serialization finishes before opening the path, so unsupported non-UTF-8
/// paths cannot create an empty output file. Existing files are never overwritten.
/// I/O failures while writing or flushing are returned and may leave a partial file.
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
mod tests;

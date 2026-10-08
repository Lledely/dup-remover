//! Argument definitions and checked parsing of file sizes and worker counts.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use dup_remover::ScanOptions;

/// Top-level command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = "dup-remover",
    version,
    about = "Поиск файлов с одинаковым содержимым",
    propagate_version = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// Operation selected by the user.
    #[command(subcommand)]
    pub command: Command,
}

/// Supported command-line operations.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Рекурсивно найти дубликаты и показать отчёт
    Scan(ScanArgs),
}

/// Paths and resource limits for a directory scan.
#[derive(Debug, Args)]
pub struct ScanArgs {
    /// Папка для сканирования
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Минимальный размер: байты или целое число с B/KB/MB/GB/KiB/MiB/GiB
    #[arg(long, value_name = "SIZE", default_value = "0", value_parser = parse_min_size)]
    pub min_size: u64,

    /// Число потоков (по умолчанию — доступное системе)
    #[arg(long, value_name = "N", default_value_t = default_jobs(), value_parser = parse_jobs)]
    pub jobs: usize,

    /// Сохранить полный JSON-отчёт в новый файл
    #[arg(long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

fn default_jobs() -> usize {
    ScanOptions::default().jobs
}

/// Parse a positive worker count that fits the current platform's `usize`.
///
/// Zero, negative values, non-integers, and overflow are rejected before scanning.
fn parse_jobs(value: &str) -> Result<usize, String> {
    let jobs = value
        .parse::<usize>()
        .map_err(|_| "число потоков должно быть положительным целым числом".to_owned())?;
    if jobs == 0 {
        return Err("число потоков должно быть больше нуля".to_owned());
    }
    Ok(jobs)
}

/// Convert an integer byte count or supported size suffix into bytes.
///
/// Decimal units use powers of 1000 and binary units use powers of 1024; suffixes
/// are case-insensitive. Missing digits, unsupported suffixes, and arithmetic
/// overflow return a user-facing validation error without truncation or wrapping.
fn parse_min_size(value: &str) -> Result<u64, String> {
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return Err("размер должен начинаться с целого неотрицательного числа".to_owned());
    }

    let number = value[..digits]
        .parse::<u64>()
        .map_err(|_| "размер превышает диапазон u64".to_owned())?;
    let multiplier = match value[digits..].to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "KIB" => 1_024,
        "MIB" => 1_048_576,
        "GIB" => 1_073_741_824,
        _ => return Err("допустимые единицы: B, KB, MB, GB, KiB, MiB, GiB".to_owned()),
    };

    number
        .checked_mul(multiplier)
        .ok_or_else(|| "размер в байтах превышает диапазон u64".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bytes_and_decimal_and_binary_units() {
        for (value, expected) in [
            ("0", 0),
            ("42", 42),
            ("1B", 1),
            ("2KB", 2_000),
            ("3MB", 3_000_000),
            ("4GB", 4_000_000_000),
            ("2KiB", 2_048),
            ("3MiB", 3_145_728),
            ("4GiB", 4_294_967_296),
            ("1mib", 1_048_576),
        ] {
            assert_eq!(parse_min_size(value).unwrap(), expected, "{value}");
        }
        assert_eq!(parse_min_size(&u64::MAX.to_string()).unwrap(), u64::MAX);
    }

    #[test]
    fn rejects_invalid_sizes_and_overflow() {
        for value in [
            "",
            "-1",
            "+1",
            "1.5MB",
            "MB",
            "1TB",
            "1 MB",
            " 1",
            "1 ",
            "１MB",
            "18446744073709551616",
            "18446744073709551615KB",
        ] {
            assert!(parse_min_size(value).is_err(), "{value}");
        }
    }

    #[test]
    fn rejects_zero_negative_and_overflowing_job_counts() {
        assert_eq!(parse_jobs("1").unwrap(), 1);
        assert_eq!(parse_jobs("8").unwrap(), 8);
        for value in ["0", "-1", "", "many", "1.5"] {
            assert!(parse_jobs(value).is_err(), "{value}");
        }
        assert!(parse_jobs(&format!("{}0", usize::MAX)).is_err());
    }

    #[test]
    fn uses_defaults_and_accepts_scan_options() {
        let cli = Cli::try_parse_from(["dup-remover", "scan", "./sample"]).unwrap();
        let Command::Scan(args) = cli.command;
        assert_eq!(args.path, PathBuf::from("./sample"));
        assert_eq!(args.min_size, 0);
        assert!(args.jobs > 0);
        assert!(args.output.is_none());

        let cli = Cli::try_parse_from([
            "dup-remover",
            "scan",
            "./sample",
            "--min-size",
            "2MiB",
            "--jobs",
            "4",
            "--output",
            "report.json",
        ])
        .unwrap();
        let Command::Scan(args) = cli.command;
        assert_eq!(args.min_size, 2_097_152);
        assert_eq!(args.jobs, 4);
        assert_eq!(args.output, Some(PathBuf::from("report.json")));
    }
}

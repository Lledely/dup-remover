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
#[path = "../tests/unit/cli.rs"]
mod tests;

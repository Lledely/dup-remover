//! Command-line entry point for scanning directories and writing reports.

mod cli;
mod report;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use dup_remover::{ScanOptions, scan};

use cli::{Cli, Command};

/// Parse arguments and translate application errors into stderr and an exit code.
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr().lock(), "Ошибка: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Execute the requested scan, save an optional JSON report, and display the result.
///
/// JSON is created only after scanning succeeds and before reporting its filename
/// to stdout. Scan and output errors include the operation and affected path.
fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Scan(args) => {
            let options = ScanOptions {
                min_size: args.min_size,
                jobs: args.jobs,
            };
            let result = scan(&args.path, &options).map_err(|error| {
                format!("Не удалось сканировать {}: {error}", args.path.display())
            })?;

            if let Some(path) = &args.output {
                report::write_json(path, &result).map_err(|error| {
                    format!(
                        "Не удалось сохранить JSON-отчёт в {}: {error}",
                        path.display()
                    )
                })?;
            }

            report::write_output(&result, args.output.as_deref(), io::stdout().lock())
                .map_err(|error| format!("Не удалось вывести отчёт: {error}"))
        }
    }
}

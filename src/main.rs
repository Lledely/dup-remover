mod cli;
mod report;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use dup_remover::{ScanOptions, scan};

use cli::{Cli, Command};

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr().lock(), "Ошибка: {message}");
            ExitCode::FAILURE
        }
    }
}

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

            let mut stdout = io::stdout().lock();
            report::write_text(&result, &mut stdout)
                .map_err(|error| format!("Не удалось вывести отчёт: {error}"))?;
            if let Some(path) = &args.output {
                writeln!(stdout, "JSON-отчёт сохранён: {}", path.display())
                    .map_err(|error| format!("Не удалось вывести отчёт: {error}"))?;
            }
            stdout
                .flush()
                .map_err(|error| format!("Не удалось вывести отчёт: {error}"))
        }
    }
}

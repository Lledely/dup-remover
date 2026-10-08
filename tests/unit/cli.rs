//! Unit tests for private argument parsers and command defaults.

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

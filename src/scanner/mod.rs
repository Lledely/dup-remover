//! Coordinate file discovery, bounded hashing, and deterministic reporting.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{DuplicateGroup, ScanOptions, ScanReport, ScanWarning};

mod discovery;
mod hashing;
mod snapshot;

use discovery::collect_candidates;
use hashing::hash_candidates;
use snapshot::FileSnapshot;

/// A path and its metadata snapshot recorded during directory discovery.
struct Candidate {
    path: PathBuf,
    snapshot: FileSnapshot,
}

/// A candidate index paired with its digest or a recoverable file error.
struct HashOutcome {
    index: usize,
    result: io::Result<String>,
}

/// Validate the root and worker count, discover candidates, and assemble a sorted report.
///
/// Root failures stop the scan. Entry and hashing failures become warnings; groups
/// contain at least two paths and are ordered independently of worker scheduling.
pub(crate) fn scan_directory(root: &Path, options: &ScanOptions) -> io::Result<ScanReport> {
    if options.jobs == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the number of hash workers must be positive",
        ));
    }

    // symlink_metadata also rejects a symbolic link supplied as the root.
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the scan root must be a directory, not a file or symbolic link",
        ));
    }
    // An unreadable root is a fatal error; unreadable subdirectories are warnings.
    let root_entries = fs::read_dir(root)?;
    let mut report = ScanReport::default();
    let candidates = collect_candidates(root, root_entries, options.min_size, &mut report);
    let outcomes = hash_candidates(&candidates, options.jobs, root, &mut report.warnings)?;

    let mut groups = BTreeMap::<(u64, String), Vec<PathBuf>>::new();
    for outcome in outcomes {
        let candidate = &candidates[outcome.index];
        match outcome.result {
            Ok(hash) => groups
                .entry((candidate.snapshot.size, hash))
                .or_default()
                .push(candidate.path.clone()),
            Err(error) => warn(
                &mut report.warnings,
                &candidate.path,
                "cannot hash file",
                error,
            ),
        }
    }

    for ((size_bytes, hash), mut paths) in groups {
        if paths.len() >= 2 {
            paths.sort();
            report.duplicate_groups.push(DuplicateGroup {
                size_bytes,
                hash,
                paths,
            });
        }
    }
    report.warnings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.message.cmp(&right.message))
    });
    Ok(report)
}

/// Record a recoverable filesystem or worker failure with the affected path.
fn warn(warnings: &mut Vec<ScanWarning>, path: &Path, action: &str, error: io::Error) {
    warnings.push(ScanWarning {
        path: path.to_path_buf(),
        message: format!("{action}: {error}"),
    });
}

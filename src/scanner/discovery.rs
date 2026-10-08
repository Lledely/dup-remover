//! Iterative directory traversal and size-based candidate selection.

use std::collections::BTreeMap;
use std::fs::{self, ReadDir};
use std::path::Path;

use crate::ScanReport;

use super::{Candidate, snapshot::FileSnapshot, warn};

/// Walk directories iteratively and return only files sharing a size with another file.
///
/// Only the current directory holds an open ReadDir handle. Symbolic links and
/// special files are skipped, including links replacing queued directories.
/// Counters include every regular file meeting the minimum size, while recoverable
/// entry errors are appended to the report even if no hash candidate is produced.
pub(super) fn collect_candidates(
    root: &Path,
    root_entries: ReadDir,
    min_size: u64,
    report: &mut ScanReport,
) -> Vec<Candidate> {
    let mut pending = vec![root.to_path_buf()];
    let mut root_entries = Some(root_entries);
    let mut by_size = BTreeMap::<u64, Vec<Candidate>>::new();

    // Store paths, not ReadDir handles, so broad trees do not exhaust open-file
    // limits. Iterative traversal also avoids call-stack growth on deep trees.
    while let Some(directory) = pending.pop() {
        let entries = if let Some(entries) = root_entries.take() {
            entries
        } else {
            // A queued directory may have been replaced by a symbolic link.
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => continue,
                Err(error) => {
                    warn(
                        &mut report.warnings,
                        &directory,
                        "cannot read metadata",
                        error,
                    );
                    continue;
                }
            }
            match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    warn(
                        &mut report.warnings,
                        &directory,
                        "cannot read directory",
                        error,
                    );
                    continue;
                }
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warn(
                        &mut report.warnings,
                        &directory,
                        "cannot read directory entry",
                        error,
                    );
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    warn(&mut report.warnings, &path, "cannot read metadata", error);
                    continue;
                }
            };

            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() && metadata.len() >= min_size {
                report.scanned_files = report.scanned_files.saturating_add(1);
                report.scanned_bytes = report.scanned_bytes.saturating_add(metadata.len());
                by_size.entry(metadata.len()).or_default().push(Candidate {
                    path,
                    snapshot: FileSnapshot::from_metadata(&metadata),
                });
            }
        }
    }

    by_size
        .into_values()
        .filter(|files| files.len() >= 2)
        .flatten()
        .collect()
}

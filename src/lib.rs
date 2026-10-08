//! Recursive duplicate discovery with bounded parallel hashing.
//!
//! The scanner only reads regular files and skips symbolic links. Hard links are
//! reported as separate paths, so the estimated reclaimable size does not measure
//! physical disk allocation. Metadata checks reject detectable file changes, but
//! a scan is not an atomic snapshot of a directory being modified concurrently.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod scanner;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub min_size: u64,
    pub jobs: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            min_size: 0,
            jobs: std::thread::available_parallelism().map_or(1, usize::from),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub size_bytes: u64,
    pub hash: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    /// Regular files meeting the minimum size, including files with read errors.
    pub scanned_files: u64,
    /// Total size of the files counted in `scanned_files`.
    pub scanned_bytes: u64,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub warnings: Vec<ScanWarning>,
}

impl ScanReport {
    /// Number of extra copies when one file in each group is retained.
    pub fn duplicate_files(&self) -> u64 {
        self.duplicate_groups
            .iter()
            .map(|group| group.paths.len().saturating_sub(1) as u64)
            .sum()
    }

    /// Estimated space occupied by extra copies; hard links are not deduplicated.
    pub fn reclaimable_bytes(&self) -> u64 {
        self.duplicate_groups.iter().fold(0, |total, group| {
            total.saturating_add(
                group
                    .size_bytes
                    .saturating_mul(group.paths.len().saturating_sub(1) as u64),
            )
        })
    }
}

/// Find duplicate regular files recursively, without traversing symbolic links.
///
/// An invalid or unreadable root and zero worker count return an error. Failures
/// involving individual entries are included in the report's warnings instead.
/// Files with unique sizes do not need to be read. Results are sorted by size and
/// hash, paths within groups, and warning path/message for reproducibility.
pub fn scan(root: impl AsRef<Path>, options: &ScanOptions) -> io::Result<ScanReport> {
    scanner::scan_directory(root.as_ref(), options)
}

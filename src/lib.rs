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

/// Size filtering and the upper bound on concurrent hashing workers.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Inclusive minimum file size in bytes; zero includes empty files.
    pub min_size: u64,
    /// Maximum hashing workers, including the caller; must be greater than zero.
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

/// At least two regular-file paths with the same size and BLAKE3 digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Size of each file in the group, not the sum of their sizes.
    pub size_bytes: u64,
    /// Lowercase hexadecimal BLAKE3 digest of the file contents.
    pub hash: String,
    /// File paths in sorted order; hard links remain separate paths.
    pub paths: Vec<PathBuf>,
}

/// A recoverable filesystem or worker-start error encountered during a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanWarning {
    /// File or directory associated with the failed operation.
    pub path: PathBuf,
    /// Operation and underlying error, suitable for displaying in a report.
    pub message: String,
}

/// Scan statistics, reproducibly ordered duplicate groups, and recoverable errors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    /// Regular files meeting the minimum size, including files with read errors.
    pub scanned_files: u64,
    /// Total size of the files counted in `scanned_files`.
    pub scanned_bytes: u64,
    /// Duplicate groups ordered by file size and digest.
    pub duplicate_groups: Vec<DuplicateGroup>,
    /// Recoverable errors ordered by path and message.
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

    /// Estimate bytes occupied by extra copies while retaining one path per group.
    ///
    /// Hard links are not deduplicated. This logical estimate does not account for
    /// shared disk blocks or compression and saturates at `u64::MAX` on overflow.
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
///
/// # Errors
///
/// Returns an error if `jobs` is zero, the root is not a readable directory, or a
/// hashing worker panics. A symbolic link supplied as the root is also rejected.
///
#[doc = include_str!("../tests/docs/scan.md")]
pub fn scan(root: impl AsRef<Path>, options: &ScanOptions) -> io::Result<ScanReport> {
    scanner::scan_directory(root.as_ref(), options)
}

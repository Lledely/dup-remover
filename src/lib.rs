//! Shared scan options and serializable results for the duplicate finder.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

/// Scan a directory recursively. The implementation is delivered in the scanner PR.
pub fn scan(_root: impl AsRef<Path>, _options: &ScanOptions) -> io::Result<ScanReport> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the scan engine is being implemented",
    ))
}

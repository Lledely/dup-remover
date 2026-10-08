//! Snapshot comparison shared by discovery and hashing.

use std::fs::Metadata;
use std::io;
use std::time::SystemTime;

/// Comparable discovery metadata, including inode and change-time identity on Unix.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FileSnapshot {
    pub(super) size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileSnapshot {
    /// Capture length and available timestamps, plus Unix identity when supported.
    ///
    /// Unsupported timestamp fields are kept as None so snapshots remain usable
    /// on filesystems that cannot provide every metadata field.
    pub(super) fn from_metadata(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            #[cfg(unix)]
            identity: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        }
    }
}

/// Reject paths that are no longer regular files or differ from the discovery snapshot.
pub(super) fn ensure_unchanged(metadata: &Metadata, snapshot: &FileSnapshot) -> io::Result<()> {
    if !metadata.is_file() || FileSnapshot::from_metadata(metadata) != *snapshot {
        return Err(changed_file());
    }
    Ok(())
}

/// Create the shared diagnostic for a candidate changed during scanning.
pub(super) fn changed_file() -> io::Error {
    io::Error::other("file changed while scanning; skipped")
}

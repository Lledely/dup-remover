use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, ReadDir};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::SystemTime;

use crate::{DuplicateGroup, ScanOptions, ScanReport, ScanWarning};

#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileSnapshot {
    fn from_metadata(metadata: &Metadata) -> Self {
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

struct Candidate {
    path: PathBuf,
    snapshot: FileSnapshot,
}

struct HashOutcome {
    index: usize,
    result: io::Result<String>,
}

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

fn collect_candidates(
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

fn hash_candidates(
    candidates: &[Candidate],
    jobs: usize,
    root: &Path,
    warnings: &mut Vec<ScanWarning>,
) -> io::Result<Vec<HashOutcome>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = jobs.min(candidates.len());
    let next_index = AtomicUsize::new(0);
    thread::scope(|scope| {
        let mut handles = Vec::new();
        // The caller is one worker, keeping total active hash workers <= jobs.
        for worker in 1..worker_count {
            let next_index = &next_index;
            match thread::Builder::new()
                .name(format!("dup-remover-{worker}"))
                .spawn_scoped(scope, move || hash_worker(candidates, next_index))
            {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    warn(
                        warnings,
                        root,
                        "cannot start worker; using available workers",
                        error,
                    );
                    break;
                }
            }
        }

        let mut outcomes = hash_worker(candidates, &next_index);
        for handle in handles {
            outcomes.extend(
                handle
                    .join()
                    .map_err(|_| io::Error::other("a hash worker panicked"))?,
            );
        }
        Ok(outcomes)
    })
}

fn hash_worker(candidates: &[Candidate], next_index: &AtomicUsize) -> Vec<HashOutcome> {
    let mut outcomes = Vec::new();
    loop {
        let index = next_index.fetch_add(1, Ordering::Relaxed);
        let Some(candidate) = candidates.get(index) else {
            break;
        };
        outcomes.push(HashOutcome {
            index,
            result: hash_file(candidate),
        });
    }
    outcomes
}

fn hash_file(candidate: &Candidate) -> io::Result<String> {
    ensure_unchanged(&fs::symlink_metadata(&candidate.path)?, &candidate.snapshot)?;
    let mut file = File::open(&candidate.path)?;
    ensure_unchanged(&file.metadata()?, &candidate.snapshot)?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes_read = 0_u64;
    // Read at most one byte beyond the recorded size to detect growth without
    // chasing a file that another process keeps appending to indefinitely.
    let mut reader = file
        .by_ref()
        .take(candidate.snapshot.size.saturating_add(1));
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if read == 0 {
            break;
        }
        bytes_read += read as u64;
        hasher.update(&buffer[..read]);
    }

    if bytes_read != candidate.snapshot.size {
        return Err(changed_file());
    }
    ensure_unchanged(&file.metadata()?, &candidate.snapshot)?;
    ensure_unchanged(&fs::symlink_metadata(&candidate.path)?, &candidate.snapshot)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn ensure_unchanged(metadata: &Metadata, snapshot: &FileSnapshot) -> io::Result<()> {
    if !metadata.is_file() || FileSnapshot::from_metadata(metadata) != *snapshot {
        return Err(changed_file());
    }
    Ok(())
}

fn changed_file() -> io::Error {
    io::Error::other("file changed while scanning; skipped")
}

fn warn(warnings: &mut Vec<ScanWarning>, path: &Path, action: &str, error: io::Error) {
    warnings.push(ScanWarning {
        path: path.to_path_buf(),
        message: format!("{action}: {error}"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: PathBuf) -> Candidate {
        Candidate {
            snapshot: FileSnapshot::from_metadata(&fs::symlink_metadata(&path).unwrap()),
            path,
        }
    }

    #[test]
    fn rejects_file_changed_after_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("changing.txt");
        fs::write(&path, b"before").unwrap();
        let candidate = candidate(path.clone());
        fs::write(path, b"a different size").unwrap();

        assert!(
            hash_file(&candidate)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
    }

    #[test]
    fn hashes_more_than_one_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.bin");
        let contents = vec![42; 3 * 64 * 1024 + 17];
        fs::write(&path, &contents).unwrap();

        assert_eq!(
            hash_file(&candidate(path)).unwrap(),
            blake3::hash(&contents).to_hex().to_string()
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_replaced_file_even_when_size_matches() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original");
        let replacement = directory.path().join("replacement");
        fs::write(&path, b"same").unwrap();
        fs::write(&replacement, b"same").unwrap();
        let candidate = candidate(path.clone());
        fs::rename(replacement, path).unwrap();

        assert!(
            hash_file(&candidate)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_file_replaced_by_symbolic_link() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original");
        let target = directory.path().join("target");
        fs::write(&path, b"same").unwrap();
        fs::write(&target, b"same").unwrap();
        let candidate = candidate(path.clone());
        fs::remove_file(&path).unwrap();
        symlink(target, path).unwrap();

        assert!(
            hash_file(&candidate)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
    }
}

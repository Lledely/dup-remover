//! Bounded parallel BLAKE3 hashing without loading whole files into memory.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crate::ScanWarning;

use super::{
    Candidate, HashOutcome,
    snapshot::{changed_file, ensure_unchanged},
    warn,
};

/// Hash candidates using at most `jobs` workers, including the calling thread.
///
/// The atomic index assigns each candidate once without sharing file handles.
/// Worker creation failures become warnings and reduce concurrency; a worker
/// panic returns an error. Individual file failures remain in their outcomes.
pub(super) fn hash_candidates(
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

/// Claim candidate indices atomically and collect this worker's hashing outcomes.
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

/// Stream a candidate through BLAKE3 while checking its discovery snapshot.
///
/// Path and open-handle metadata are checked before and after reading. Reading
/// stops at the recorded length plus one byte so a growing file cannot keep the
/// scan busy indefinitely. Detectable replacement, truncation, or growth returns
/// an error; these checks do not provide an atomic filesystem snapshot.
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

#[cfg(test)]
mod tests;

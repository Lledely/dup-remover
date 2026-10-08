use std::path::PathBuf;

use super::super::snapshot::FileSnapshot;
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

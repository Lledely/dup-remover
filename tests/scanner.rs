use std::fs;
use std::io;
use std::path::Path;

use dup_remover::{ScanOptions, ScanReport, scan};
use tempfile::TempDir;

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn scan_with(root: &Path, min_size: u64, jobs: usize) -> ScanReport {
    scan(root, &ScanOptions { min_size, jobs }).unwrap()
}

#[test]
fn discovers_nested_duplicates_and_separates_equal_size_contents() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "z.txt", b"rust");
    write(root, "nested/a.txt", b"rust");
    write(root, "nested/deeper/копия.txt", b"rust");
    write(root, "different.txt", b"code");
    write(root, "unique.txt", b"unique length");

    let report = scan_with(root, 0, 4);

    assert_eq!(report.scanned_files, 5);
    assert_eq!(report.scanned_bytes, 4 * 4 + 13);
    assert!(report.warnings.is_empty());
    assert_eq!(report.duplicate_groups.len(), 1);
    let group = &report.duplicate_groups[0];
    assert_eq!(group.size_bytes, 4);
    assert_eq!(group.hash, blake3::hash(b"rust").to_hex().to_string());
    let mut expected = vec![
        root.join("z.txt"),
        root.join("nested/a.txt"),
        root.join("nested/deeper/копия.txt"),
    ];
    expected.sort();
    assert_eq!(group.paths, expected);
    assert_eq!(report.duplicate_files(), 2);
    assert_eq!(report.reclaimable_bytes(), 8);
}

#[test]
fn handles_empty_directories_and_empty_files() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("empty-directory")).unwrap();
    assert_eq!(scan_with(root, 0, 1), ScanReport::default());

    write(root, "empty-a", b"");
    write(root, "empty-directory/empty-b", b"");
    let report = scan_with(root, 0, 2);
    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.scanned_bytes, 0);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(
        report.duplicate_groups[0].hash,
        blake3::hash(b"").to_hex().to_string()
    );
    assert_eq!(report.duplicate_files(), 1);
    assert_eq!(report.reclaimable_bytes(), 0);
}

#[test]
fn minimum_size_is_inclusive_and_filters_counters() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "empty", b"");
    write(root, "small-a", b"abc");
    write(root, "small-b", b"abc");
    write(root, "boundary-a", b"rust");
    write(root, "boundary-b", b"rust");
    write(root, "larger", b"longer");

    let report = scan_with(root, 4, 2);
    assert_eq!(report.scanned_files, 3);
    assert_eq!(report.scanned_bytes, 14);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.duplicate_groups[0].size_bytes, 4);
    assert!(report.warnings.is_empty());
    assert_eq!(scan_with(root, u64::MAX, 2), ScanReport::default());
}

#[test]
fn validates_root_and_worker_count() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let options = ScanOptions {
        min_size: 0,
        jobs: 1,
    };
    assert_eq!(
        scan(root.join("missing"), &options).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    write(root, "file", b"contents");
    assert_eq!(
        scan(root.join("file"), &options).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        scan(
            root,
            &ScanOptions {
                min_size: 0,
                jobs: 0
            }
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn single_file_and_equal_size_unique_files_have_no_duplicate_groups() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "single", b"one");
    let report = scan_with(root, 0, 8);
    assert_eq!(report.scanned_files, 1);
    assert!(report.duplicate_groups.is_empty());

    write(root, "different", b"two");
    let report = scan_with(root, 0, 8);
    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.scanned_bytes, 6);
    assert!(report.duplicate_groups.is_empty());
    assert!(report.warnings.is_empty());
}

#[test]
fn parallel_and_serial_reports_are_identical_and_sorted() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    for index in (0..24).rev() {
        let contents = vec![(index % 4) as u8; (index % 3 + 1) * 73_001];
        write(
            root,
            &format!("nested-{}/file-{index}", index % 5),
            &contents,
        );
    }

    let serial = scan_with(root, 0, 1);
    assert_eq!(serial.duplicate_groups.len(), 12);
    assert_eq!(serial.duplicate_files(), 12);
    for jobs in [2, 4, 32] {
        assert_eq!(scan_with(root, 0, jobs), serial);
    }
    assert!(serial.duplicate_groups.windows(2).all(|groups| {
        (groups[0].size_bytes, &groups[0].hash) <= (groups[1].size_bytes, &groups[1].hash)
    }));
    assert!(
        serial
            .duplicate_groups
            .iter()
            .all(|group| { group.paths.windows(2).all(|paths| paths[0] <= paths[1]) })
    );
}

#[test]
fn counts_hard_links_as_paths_and_reports_logical_size() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "original", b"linked");
    fs::hard_link(root.join("original"), root.join("hard-link")).unwrap();

    let report = scan_with(root, 0, 2);
    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.scanned_bytes, 12);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.duplicate_files(), 1);
    assert_eq!(report.reclaimable_bytes(), 6);
}

#[test]
fn scans_broad_directory_tree_completely() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let directory_count = 384;
    for index in 0..directory_count {
        write(root, &format!("directory-{index}/copy"), b"shared contents");
    }

    let report = scan_with(root, 0, 4);
    assert_eq!(report.scanned_files, directory_count);
    assert_eq!(report.scanned_bytes, directory_count * 15);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(
        report.duplicate_groups[0].paths.len() as u64,
        directory_count
    );
    assert!(report.warnings.is_empty());
}

#[cfg(unix)]
#[test]
fn skips_file_directory_and_broken_symbolic_links_and_rejects_link_root() {
    use std::os::unix::fs::symlink;

    let directory = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "original", b"same");
    write(outside.path(), "outside-a", b"same");
    write(outside.path(), "outside-b", b"same");
    symlink(root.join("original"), root.join("file-link")).unwrap();
    symlink(outside.path(), root.join("directory-link")).unwrap();
    symlink(root.join("missing"), root.join("broken-link")).unwrap();
    symlink(root, root.join("cycle")).unwrap();

    let report = scan_with(root, 0, 4);
    assert_eq!(report.scanned_files, 1);
    assert!(report.duplicate_groups.is_empty());
    assert!(report.warnings.is_empty());
    assert_eq!(
        scan(root.join("cycle"), &ScanOptions::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[cfg(unix)]
#[test]
fn reports_unreadable_candidate_and_continues() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "unreadable", b"same");
    write(root, "readable-a", b"same");
    write(root, "readable-b", b"same");
    let path = root.join("unreadable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    // Root and some sandbox accounts can bypass mode bits; do not assert an
    // unreadable fixture when the environment can still open it.
    if fs::File::open(&path).is_ok() {
        return;
    }

    let report = scan_with(root, 0, 4);
    assert_eq!(report.scanned_files, 3);
    assert_eq!(report.scanned_bytes, 12);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.duplicate_groups[0].paths.len(), 2);
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].path, path);
    assert_eq!(scan_with(root, 0, 1), report);
}

#[cfg(unix)]
#[test]
fn reports_unreadable_subdirectory_but_errors_on_unreadable_root() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let root = directory.path();
    write(root, "readable-a", b"same");
    write(root, "readable-b", b"same");
    write(root, "blocked/inside", b"same");
    let blocked = root.join("blocked");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&blocked).is_ok() {
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }

    let report = scan_with(root, 0, 2);
    let blocked_result = scan(&blocked, &ScanOptions::default());
    // Restore directory access so TempDir can remove the contents even if an
    // assertion later fails.
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.duplicate_groups.len(), 1);
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].path, blocked);
    assert_eq!(
        blocked_result.unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}

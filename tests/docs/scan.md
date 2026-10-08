# Examples

```
use dup_remover::{ScanOptions, scan};

let directory = tempfile::tempdir()?;
std::fs::write(directory.path().join("original.txt"), b"rust")?;
std::fs::write(directory.path().join("copy.txt"), b"rust")?;
let report = scan(directory.path(), &ScanOptions { min_size: 0, jobs: 2 })?;
assert_eq!(report.duplicate_files(), 1);
assert_eq!(report.reclaimable_bytes(), 4);
# Ok::<(), std::io::Error>(())
```

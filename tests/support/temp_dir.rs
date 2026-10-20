use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn create_temp_dir(base: &Path) -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let mut path = base.to_owned();
    loop {
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let mut name = base.as_os_str().to_os_string();
                name.push(format!("-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
                path = PathBuf::from(name);
            }
            Err(error) => panic!("cannot create temporary directory {path:?}: {error}"),
        }
    }
}

#[test]
fn retries_existing_paths_and_keeps_parallel_allocations_distinct() {
    let base =
        std::env::temp_dir().join(format!("imara-diff-dir-regression-{}", std::process::id()));
    let first = create_temp_dir(&base);
    fs::write(first.join("sentinel"), b"do not replace").unwrap();
    let second = std::panic::catch_unwind(|| create_temp_dir(&first));
    assert_eq!(fs::read(first.join("sentinel")).unwrap(), b"do not replace");
    fs::remove_dir_all(&first).unwrap();
    let second = second.expect("an existing directory must cause a retry, not a panic");
    assert_ne!(first, second);
    fs::remove_dir_all(second).unwrap();

    let paths = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..32)
            .map(|_| scope.spawn(|| create_temp_dir(&base)))
            .collect();
        handles
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    let distinct: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(distinct.len(), paths.len());
    assert!(paths.iter().all(|path| path.is_dir()));
    for path in paths {
        fs::remove_dir_all(path).unwrap();
    }
}

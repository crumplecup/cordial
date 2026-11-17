//! The key of a run hashes the project's files; what a file hashed to is remembered so that one
//! that has not changed is not read again, and one that may have is.

use cordial::testing::run_cache_digest;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a directory");
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"keyed\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");
    std::fs::create_dir(root.path().join("src")).expect("src");
    std::fs::write(root.path().join("src/lib.rs"), "pub fn a() {}\n").expect("a source");
    root
}

fn memo_of(root: &Path) -> PathBuf {
    root.join("memo").join("file-digests.json")
}

fn set_age(path: &Path, age: Duration) {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the file");
    file.set_modified(SystemTime::now() - age).expect("a time");
}

#[test]
fn a_changed_file_changes_the_key_with_the_memo_as_without() {
    let root = project();
    let source = root.path().join("src/lib.rs");
    let elsewhere = tempfile::tempdir().expect("a directory");
    let memo = memo_of(elsewhere.path());
    let before = run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert_eq!(before, run_cache_digest(root.path(), None).expect("a key"));
    std::fs::write(&source, "pub fn b() {}\n").expect("an edit of the same size");
    let after = run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert_ne!(before, after, "a file just written was not hashed again");
    assert_eq!(after, run_cache_digest(root.path(), None).expect("a key"));
}

#[test]
fn a_file_that_has_stood_still_is_not_read_again() {
    let root = project();
    // Listing the packages writes the lock file, which is then as old as the rest.
    run_cache_digest(root.path(), None).expect("a key");
    let aged = ["src/lib.rs", "Cargo.toml", "Cargo.lock"];
    for file in aged {
        set_age(&root.path().join(file), Duration::from_secs(3600));
    }
    let source = root.path().join("src/lib.rs");
    let time = std::fs::metadata(&source)
        .and_then(|meta| meta.modified())
        .expect("a time");
    let memo = memo_of(root.path());
    let before = run_cache_digest(root.path(), Some(&memo)).expect("a key");
    // The same size and the same time: the memo is taken to hold, so the contents are not read.
    // That is the assumption that makes a run that reuses everything cheap.
    std::fs::write(&source, "pub fn b() {}\n").expect("an edit");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .and_then(|file| file.set_modified(time))
        .expect("the time put back");
    assert_eq!(
        before,
        run_cache_digest(root.path(), Some(&memo)).expect("a key")
    );
    // A time that moved is not.
    set_age(&source, Duration::from_secs(1800));
    assert_ne!(
        before,
        run_cache_digest(root.path(), Some(&memo)).expect("a key")
    );
}

#[test]
fn an_unchanged_file_keeps_the_key_and_the_memo() {
    let root = project();
    // Listing the packages writes the lock file, which is then as old as the rest.
    run_cache_digest(root.path(), None).expect("a key");
    for file in ["src/lib.rs", "Cargo.toml", "Cargo.lock"] {
        set_age(&root.path().join(file), Duration::from_secs(3600));
    }
    let memo = memo_of(root.path());
    let first = run_cache_digest(root.path(), Some(&memo)).expect("a key");
    let second = run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert_eq!(first, second);
    let remembered = std::fs::read_to_string(&memo).expect("a memo");
    run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert_eq!(
        remembered,
        std::fs::read_to_string(&memo).expect("a memo"),
        "rewritten with nothing new"
    );
}

#[test]
fn a_file_that_is_gone_is_forgotten() {
    let root = project();
    let extra = root.path().join("src/extra.rs");
    std::fs::write(&extra, "// x\n").expect("a file");
    let memo = memo_of(root.path());
    run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert!(
        std::fs::read_to_string(&memo)
            .expect("a memo")
            .contains("extra.rs")
    );
    std::fs::remove_file(&extra).expect("removed");
    run_cache_digest(root.path(), Some(&memo)).expect("a key");
    assert!(
        !std::fs::read_to_string(&memo)
            .expect("a memo")
            .contains("extra.rs")
    );
}

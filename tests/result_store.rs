//! The results processes of one run hand to each other: what one writes the next finds, by name,
//! after the first has ended.

use cordial::ResultStore;
use miette::{IntoDiagnostic, Result};

#[test]
fn a_result_put_is_found_by_its_kind_and_name_before_and_after_it_is_written() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let store = ResultStore::open(&directory.path().join("results.redb")).into_diagnostic()?;
    assert_eq!(store.get("evidence", "a::f"), None, "nothing yet");
    store.put("evidence", "a::f", b"one");
    assert_eq!(
        store.get("evidence", "a::f").as_deref(),
        Some(&b"one"[..]),
        "found before it is written"
    );
    store.flush();
    assert_eq!(
        store.get("evidence", "a::f").as_deref(),
        Some(&b"one"[..]),
        "and after"
    );
    assert_eq!(store.get("other", "a::f"), None, "a kind of its own");
    store.put("evidence", "a::f", b"two");
    assert_eq!(
        store.get("evidence", "a::f").as_deref(),
        Some(&b"two"[..]),
        "replaced"
    );
    Ok(())
}

#[test]
fn what_a_process_wrote_is_there_for_the_one_that_comes_after_it() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let path = directory.path().join("results.redb");
    {
        let first = ResultStore::open(&path).into_diagnostic()?;
        first.put("evidence", "a::f", b"1");
        first.put("evidence", "a::g", b"2");
        first.put("context", "a::f", b"3");
        // Nothing is flushed: the end of the process does it.
    }
    let second = ResultStore::open(&path).into_diagnostic()?;
    assert_eq!(second.get("evidence", "a::g").as_deref(), Some(&b"2"[..]));
    assert_eq!(second.count(), 3, "all three");
    let done = second.names("evidence");
    assert_eq!(done.len(), 2, "two of that kind: {done:?}");
    assert!(done.contains("a::f") && done.contains("a::g"));
    second.put("evidence", "a::h", b"4");
    let done = second.names("evidence");
    assert!(done.contains("a::h"), "and what it has put and not written");
    Ok(())
}

#[test]
fn the_names_of_a_kind_are_those_of_that_kind_alone() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let store = ResultStore::open(&directory.path().join("results.redb")).into_diagnostic()?;
    for name in ["b", "a", "c"] {
        store.put("evidence", name, b"x");
        store.put("evidence-extra", name, b"y");
        store.put("aaa", name, b"z");
    }
    store.flush();
    let names = store.names("evidence");
    assert_eq!(
        names.len(),
        3,
        "not those of a kind that begins the same way: {names:?}"
    );
    Ok(())
}

#[test]
fn many_threads_can_put_at_once_and_the_size_on_disk_grows_with_the_work() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let store = ResultStore::open(&directory.path().join("results.redb")).into_diagnostic()?;
    let before = store.bytes_on_disk();
    std::thread::scope(|scope| {
        for thread in 0..8 {
            let store = &store;
            scope.spawn(move || {
                for index in 0..500 {
                    store.put("evidence", &format!("t{thread}::f{index}"), &[7_u8; 600]);
                }
            });
        }
    });
    assert_eq!(store.count(), 4000, "every one of them");
    assert!(
        store.bytes_on_disk() > before + 1_000_000,
        "the file is bigger by what was written: {before} then {}",
        store.bytes_on_disk()
    );
    Ok(())
}

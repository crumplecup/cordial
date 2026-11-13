//! What a store holds is read from its root, so every artifact is counted by where
//! it is put, and the profile's storage values come from the places that know them.

use cordial::{StoreLayout, store_usage};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

fn write(root: &Path, relative: &str, bytes: usize) -> Result<()> {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap_or(root)).into_diagnostic()?;
    std::fs::write(path, vec![b'x'; bytes]).into_diagnostic()
}

#[test]
fn a_store_is_counted_by_its_top_level_entries() -> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    write(scratch.path(), "cache/run/abc/ir/a.ir.zst", 1000)?;
    write(scratch.path(), "cache/run/abc/inputs.json", 24)?;
    write(scratch.path(), "findings/summary.md", 300)?;
    write(scratch.path(), "findings/deep/one.csv", 12)?;
    write(scratch.path(), "ledger.json", 7)?;

    let usage: Vec<_> = store_usage(scratch.path())
        .into_iter()
        .map(|entry| (entry.name().clone(), entry.files(), entry.bytes()))
        .collect();
    assert_eq!(
        usage,
        vec![
            ("cache".to_owned(), 2, 1024),
            ("findings".to_owned(), 2, 312),
            ("ledger.json".to_owned(), 1, 7),
        ]
    );
    Ok(())
}

#[test]
fn a_missing_store_holds_nothing() {
    assert!(store_usage(Path::new("/nonexistent/store/root")).is_empty());
    let _ = StoreLayout::from_root("/nonexistent", "x");
}

#[test]
fn the_values_come_from_the_places_that_know_them() -> Result<()> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let read = |file: &str| std::fs::read_to_string(src.join(file)).into_diagnostic();
    assert!(
        read("session/run.rs")?.contains("storage::record_store"),
        "a run records what its store holds"
    );
    assert!(
        read("loader/database/projection.rs")?.contains("storage::record_database"),
        "a projection records what the database holds"
    );
    assert!(
        read("loader/database/context_crates.rs")?.contains("storage::record_context_copy"),
        "a context records what its copy holds"
    );
    Ok(())
}

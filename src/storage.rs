//! What cordial keeps, as numbers a profile carries.
//!
//! Three kinds of storage matter, and each is read from one place, not from every
//! function that writes:
//!
//! - **What is persisted.** A store has one root, and everything cordial writes
//!   lives under it, so [`store_usage`] walks the root once and reports each top
//!   level entry's files and bytes. A new artifact is counted by where it is put,
//!   with no call to add.
//! - **What a session keeps in memory.** The Salsa database's memos, per query,
//!   and the copies a cfg context makes of the workspace.
//! - **The heap.** hotpath's allocator counts that for every function, with the
//!   `hotpath-alloc` feature; nothing here repeats it.
//!
//! The values go into hotpath's report through `val!`, which costs nothing without
//! the `hotpath` feature (the macro expands to a no-op, and the key is not built).

use std::path::Path;

use tracing::instrument;

use crate::StoreLayout;

/// The files and bytes under one top-level entry of a store.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct StoreUsage {
    /// The entry's name: a directory such as `cache`, or a file such as `ledger.json`.
    name: String,
    /// How many files it holds (one, for a file).
    #[getter(copy)]
    files: u64,
    /// Their size in bytes.
    #[getter(copy)]
    bytes: u64,
}

/// What each top-level entry of the store at `root` holds, sorted by name. A root
/// that does not exist holds nothing.
#[instrument(level = "debug", skip(root))]
#[hotpath::measure]
pub fn store_usage(root: &Path) -> Vec<StoreUsage> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut usage: Vec<StoreUsage> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let (files, bytes) = walkdir::WalkDir::new(entry.path())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|inner| inner.file_type().is_file())
                .fold((0_u64, 0_u64), |(files, bytes), inner| {
                    (
                        files + 1,
                        bytes + inner.metadata().map_or(0, |meta| meta.len()),
                    )
                });
            StoreUsage::new(
                entry.file_name().to_string_lossy().into_owned(),
                files,
                bytes,
            )
        })
        .collect();
    usage.sort_by(|a, b| a.name().cmp(b.name()));
    usage
}

/// Put the store's usage in the profile, one pair of values per top-level entry.
#[instrument(level = "debug", skip(layout))]
#[hotpath::measure]
pub(crate) fn record_store(layout: &StoreLayout) {
    // Walking the store costs a little; only a profile reads the result.
    if !cfg!(feature = "hotpath") {
        return;
    }
    for entry in store_usage(layout.root()) {
        hotpath::val!(format!("storage.store.{}.bytes", entry.name())).set(&entry.bytes());
        hotpath::val!(format!("storage.store.{}.files", entry.name())).set(&entry.files());
    }
}

/// Put what a cfg context's copy of the workspace holds in the profile.
#[instrument(level = "debug")]
#[hotpath::measure]
pub(crate) fn record_context_copy(context: &str, files: usize, bytes: usize, crates: usize) {
    hotpath::val!(format!("storage.context.{context}.files")).set(&files);
    hotpath::val!(format!("storage.context.{context}.bytes")).set(&bytes);
    hotpath::val!(format!("storage.context.{context}.crates")).set(&crates);
}

/// Put what the Salsa database holds in the profile: for each query, how many
/// results it has memoised and how large they are, and the same for its inputs.
/// It needs the database exclusively and walks every memo, so it runs only when a
/// profile is being recorded.
#[cfg(feature = "hotpath")]
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub(crate) fn record_database(db: &mut dyn salsa::Database) {
    let info = db.memory_usage();
    for (name, query) in &info.queries {
        hotpath::val!(format!("storage.salsa.query.{name}.memos")).set(&query.count());
        hotpath::val!(format!("storage.salsa.query.{name}.bytes"))
            .set(&(query.size_of_fields() + query.size_of_metadata()));
    }
    for input in &info.structs {
        let name = input.debug_name();
        hotpath::val!(format!("storage.salsa.input.{name}.count")).set(&input.count());
        hotpath::val!(format!("storage.salsa.input.{name}.bytes"))
            .set(&(input.size_of_fields() + input.size_of_metadata()));
    }
}

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

/// What the Salsa database holds for one query (its memoised results) or one struct ingredient
/// (an input, an interned identity or a tracked struct).
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct SalsaUsage {
    /// The query or struct's name.
    name: String,
    /// Memoised results, or slots in use.
    #[getter(copy)]
    count: usize,
    /// Bytes of the values or fields, as Salsa measures them.
    #[getter(copy)]
    fields_bytes: usize,
    /// Bytes of Salsa's own records for them: memo headers, dependency edges, ownership.
    #[getter(copy)]
    metadata_bytes: usize,
    /// For a struct ingredient: slots a page has initialised, whether or not a value is still in
    /// them. Salsa keeps a slot after its value is deleted, so this is what the ingredient has
    /// claimed, and it exceeds `count` by what was released but not given back. Zero for a query.
    #[getter(copy)]
    slots_claimed: usize,
    /// For a query: memos that hold their value in memory. The others are headers only (a value
    /// dropped, spilled, or never decoded), and still count in `count` and `metadata_bytes`.
    /// Zero for a struct ingredient.
    #[getter(copy)]
    values_resident: usize,
}

/// What the Salsa database holds, per query and per struct ingredient.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct SalsaFootprint {
    queries: Vec<SalsaUsage>,
    structs: Vec<SalsaUsage>,
}

/// What `db` holds. It needs the database exclusively and walks every memo, so it is for a profile
/// or a measurement, not a hot path.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub(crate) fn salsa_footprint(db: &mut dyn salsa::Database) -> SalsaFootprint {
    let mut resident: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    salsa::Database::values_in_memory(db, false, true, &[], &mut |name, held| {
        *resident.entry(name).or_default() += usize::try_from(held).unwrap_or(usize::MAX);
    });
    let info = db.memory_usage();
    let mut queries: Vec<SalsaUsage> = info
        .queries
        .iter()
        .map(|(name, query)| {
            SalsaUsage::new(
                (*name).to_owned(),
                query.count(),
                query.size_of_fields(),
                query.size_of_metadata(),
                0,
                resident.get(name).copied().unwrap_or(0),
            )
        })
        .collect();
    queries.sort_by(|a, b| a.name().cmp(b.name()));
    let mut structs: Vec<SalsaUsage> = info
        .structs
        .iter()
        .map(|slots| {
            let claimed = slots.page_info().map_or(0, |pages| {
                (pages.page_count() * pages.page_capacity()).saturating_sub(pages.excess_capacity())
            });
            SalsaUsage::new(
                slots.debug_name().to_owned(),
                slots.count(),
                slots.size_of_fields(),
                slots.size_of_metadata(),
                claimed,
                0,
            )
        })
        .collect();
    structs.sort_by(|a, b| a.name().cmp(b.name()));
    SalsaFootprint::new(queries, structs)
}

/// Put what the Salsa database holds in the profile: for each query, how many
/// results it has memoised and how large they are, and the same for its inputs.
/// It needs the database exclusively and walks every memo, so it runs only when a
/// profile is being recorded.
#[cfg(feature = "hotpath")]
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub(crate) fn record_database(db: &mut dyn salsa::Database) {
    let footprint = salsa_footprint(db);
    for query in footprint.queries() {
        let name = query.name();
        hotpath::val!(format!("storage.salsa.query.{name}.memos")).set(&query.count());
        hotpath::val!(format!("storage.salsa.query.{name}.bytes"))
            .set(&(query.fields_bytes() + query.metadata_bytes()));
    }
    for input in footprint.structs() {
        let name = input.name();
        hotpath::val!(format!("storage.salsa.input.{name}.count")).set(&input.count());
        hotpath::val!(format!("storage.salsa.input.{name}.bytes"))
            .set(&(input.fields_bytes() + input.metadata_bytes()));
    }
}

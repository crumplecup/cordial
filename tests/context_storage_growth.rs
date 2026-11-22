//! What the Salsa database still holds once every cfg context has finished, as the number of
//! contexts grows. Retirement releases the contexts' payloads; this reports what it leaves behind
//! (inputs, interned identities, memo headers), so the next reduction is chosen from numbers.
#![cfg(feature = "hotpath")]

use std::collections::BTreeMap;
use std::path::Path;

use cordial::{
    CfgPlanning, CrateTarget, SalsaFootprint, SemanticCargoOptions, SemanticContextKey,
    SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

/// The per-file queries whose values a finished context's copied files release.
const RELEASED: &[&str] = &["extract_file", "real_span_map"];

/// What one ingredient holds: (count, bytes, claimed slots, resident values).
type Row = (usize, usize, usize, usize);

/// Every struct ingredient and query, by name.
type Table = BTreeMap<String, Row>;

/// A persisted per-file query, whose values stay resident when there is nowhere to spill them.
const KEPT_WITHOUT_A_STORE: &str = "observability_file";

/// How many more values of `name` are resident after the most contexts than after the fewest.
fn resident_growth(tables: &[Table], name: &str) -> i64 {
    let resident = |table: &Table| table.get(name).map_or(0, |row| row.3) as i64;
    match (tables.first(), tables.last()) {
        (Some(few), Some(many)) => resident(many) - resident(few),
        _ => 0,
    }
}

/// How many files the crate has: each context's gates are in every one, so each context copies
/// every file.
const FILES: usize = 8;

fn module(index: usize, contexts: usize) -> String {
    let mut text = format!(
        "pub struct Widget{index} {{ pub size: u32 }}\n\
         impl Widget{index} {{ pub fn new() -> Self {{ Self {{ size: 0 }} }} }}\n\
         pub enum Choice{index} {{ A, B(u32) }}\n\
         pub fn helper{index}() -> Widget{index} {{ Widget{index}::new() }}\n"
    );
    for context in 0..contexts {
        text.push_str(&format!(
            "#[cfg(c{context})]\npub fn gated_{index}_{context}() -> Widget{index} {{ helper{index}() }}\n"
        ));
    }
    text
}

fn footprint_after(root: &Path, contexts: usize) -> Result<SalsaFootprint> {
    let keys = (0..contexts)
        .map(|context| SemanticContextKey::enabling([format!("c{context}")]))
        .collect();
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(keys)
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    session
        .project(&CrateTarget::new("grown", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(session.salsa_footprint())
}

fn fixture(contexts: usize) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='grown'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let mods: String = (0..FILES)
        .map(|index| format!("pub mod m{index};\n"))
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), mods).into_diagnostic()?;
    for index in 0..FILES {
        std::fs::write(
            root.path().join(format!("src/m{index}.rs")),
            module(index, contexts),
        )
        .into_diagnostic()?;
    }
    Ok(root)
}

/// Every struct ingredient and query, by name, as (count, bytes, claimed slots, resident values).
fn table(footprint: &SalsaFootprint) -> Table {
    footprint
        .structs()
        .iter()
        .chain(footprint.queries())
        .map(|usage| {
            (
                usage.name().clone(),
                (
                    usage.count(),
                    usage.fields_bytes() + usage.metadata_bytes(),
                    usage.slots_claimed(),
                    usage.values_resident(),
                ),
            )
        })
        .collect()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_finished_contexts_file_values_are_released_and_the_rest_is_reported() -> Result<()> {
    cordial::init_tracing();
    let (few, many) = (1_usize, 16_usize);
    let mut tables = Vec::new();
    for contexts in [few, many] {
        let root = fixture(contexts)?;
        tables.push(table(&footprint_after(root.path(), contexts)?));
    }
    let added = (many - few) as i64;
    let mut growth: Vec<(String, i64, i64, i64, i64)> = Vec::new();
    for name in tables
        .iter()
        .flat_map(|t| t.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let at = |t: &Table| t.get(name).copied().unwrap_or_default();
        let (a, b) = (at(&tables[0]), at(&tables[1]));
        let delta = |x: usize, y: usize| y as i64 - x as i64;
        growth.push((
            name.clone(),
            delta(a.0, b.0),
            delta(a.1, b.1),
            delta(a.2, b.2),
            delta(a.3, b.3),
        ));
    }
    growth.sort_by_key(|row| std::cmp::Reverse((row.4, row.1)));
    println!(
        "{FILES} files, {few} -> {many} contexts ({} copied files added)",
        added as usize * (FILES + 1)
    );
    println!(
        "{:<44} {:>8} {:>10} {:>9} {:>11}",
        "ingredient", "d.count", "d.bytes", "d.claimed", "d.resident"
    );
    for (name, count, bytes, claimed, resident) in growth.iter().filter(|row| row.1 != 0).take(32) {
        println!("{name:<44} {count:>8} {bytes:>10} {claimed:>9} {resident:>11}");
    }
    // What a finished context's copied files computed is released with the context: these queries
    // hold no more values after sixteen contexts than after one, whatever the contexts' number.
    // (Before the per-file ownership policies each context added a value per copied file.)
    for name in RELEASED {
        let held = resident_growth(&tables, name);
        assert!(
            held <= 0,
            "{name}: {held} more values resident after {many} contexts than after {few}"
        );
    }
    // The same measure must see growth where there is some, or it proves nothing: a persisted
    // query's values go to the run cache's spill store when a context finishes, and this fixture
    // has no store, so they are kept (a final save has them). Only where the query exists.
    if tables[1].contains_key(KEPT_WITHOUT_A_STORE) {
        let held = resident_growth(&tables, KEPT_WITHOUT_A_STORE);
        assert!(
            held > 0,
            "{KEPT_WITHOUT_A_STORE} keeps its values without a spill store, so the measure must show growth"
        );
    }
    Ok(())
}

//! Seed rustdoc for minimal-workspace shadow mirror tests.

use miette::{IntoDiagnostic, WrapErr};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use cordial::{NamedRunFilter, SHADOW_ETIQUETTE, Session, SessionBuilder};

use super::CsvTable;

pub use super::minimal_fixture::write_minimal_rustdoc;

/// Seed upstream and shadow rustdoc for the url ↔ elicit_url pair.
pub fn seed_minimal_shadow_fixture(workspace: &Path, store_root: &Path) -> miette::Result<()> {
    write_minimal_rustdoc(workspace, "url", "Widget")?;
    write_minimal_rustdoc(workspace, "elicit_url", "Widget")?;

    for crate_name in ["url", "elicit_url"] {
        let source = workspace
            .join("target/doc")
            .join(format!("{crate_name}.json"));
        let crate_root = workspace.join("crates").join(crate_name);
        let local_doc = crate_root.join("doc");
        fs::create_dir_all(&local_doc)
            .into_diagnostic()
            .wrap_err("local doc dir")?;
        fs::copy(&source, local_doc.join(format!("{crate_name}.json")))
            .into_diagnostic()
            .wrap_err("copy local doc")?;
        fs::create_dir_all(store_root.join("cache/rustdoc"))
            .into_diagnostic()
            .wrap_err("store rustdoc dir")?;
        fs::copy(
            &source,
            store_root
                .join("cache/rustdoc")
                .join(format!("{crate_name}.json")),
        )
        .into_diagnostic()
        .wrap_err("copy store rustdoc")?;
    }
    Ok(())
}

/// Seed upstream rustdoc for one shadow ↔ upstream pair (`shadow-dep-{shadow}-{upstream}.json`).
pub fn seed_shadow_dep_rustdoc(
    store_root: &Path,
    shadow_crate: &str,
    upstream_crate: &str,
    type_name: &str,
) -> miette::Result<()> {
    let cache_dir = store_root.join("cache/rustdoc");
    fs::create_dir_all(&cache_dir)
        .into_diagnostic()
        .wrap_err("store rustdoc dir")?;
    let path = cache_dir.join(format!("shadow-dep-{shadow_crate}-{upstream_crate}.json"));
    super::minimal_fixture::write_minimal_rustdoc_file(&path, upstream_crate, type_name)
}

/// Run cross-crate shadow coverage on the minimal-workspace fixture.
pub fn run_cordial_shadow_coverage(
    workspace: &Path,
    store_root: &Path,
    upstream_crate: Option<&str>,
) -> miette::Result<()> {
    seed_minimal_shadow_fixture(workspace, store_root)?;

    let session = SessionBuilder::new(workspace)
        .with_store_root(store_root)
        .register(&SHADOW_ETIQUETTE)
        .build();

    let filter = match upstream_crate {
        Some(name) => NamedRunFilter::etiquettes(&["shadow"]).with_crate(name.to_string()),
        None => NamedRunFilter::etiquettes(&["shadow"]),
    };
    session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("cordial shadow run")?;
    Ok(())
}

pub const SHADOW_GAPS_KEY_COLUMNS: &[&str] = &["item_path", "gap_kind"];

pub const SHADOW_PAIR_KEY_COLUMNS: &[&str] = &[
    "item_path",
    "status",
    "verification_gap",
    "shadow_elicit_impl",
];

pub fn shadow_gaps_open(row: &HashMap<String, String>) -> bool {
    row.get("gap_kind").is_some_and(|kind| !kind.is_empty())
}

pub fn filter_shadow_gaps_by_target(table: &CsvTable, target_crate: &str) -> CsvTable {
    CsvTable {
        rows: table
            .rows
            .iter()
            .filter(|row| {
                row.get("target_crate")
                    .is_some_and(|name| name == target_crate)
            })
            .cloned()
            .collect(),
    }
}

pub fn filter_shadow_pair_by_item_path(table: &CsvTable, item_path: &str) -> CsvTable {
    CsvTable {
        rows: table
            .rows
            .iter()
            .filter(|row| row.get("item_path").is_some_and(|path| path == item_path))
            .cloned()
            .collect(),
    }
}

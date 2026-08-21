//! Run cross-crate shadow coverage on the minimal-workspace fixture.
//!
//! Built on top of `shadow_fixture.rs`'s rustdoc-writing fixtures --
//! declared alongside it (via `#[path]`) wherever these are needed.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{NamedRunFilter, SHADOW_ETIQUETTE, Session, SessionBuilder};

use crate::shadow_fixture::{seed_minimal_shadow_fixture, write_minimal_rustdoc_file};

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
    write_minimal_rustdoc_file(&path, upstream_crate, type_name)
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
        Some(name) => NamedRunFilter::etiquettes(["shadow"]).with_crate(name.to_string()),
        None => NamedRunFilter::etiquettes(["shadow"]),
    };
    session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("cordial shadow run")?;
    Ok(())
}

//! Shared test fixture helpers -- `workspace_path` resolves fixtures under
//! `tests/parity/workspaces/`, and `minimal_fixture`/`shadow_fixture`
//! build/run real cordial sessions against the `minimal-workspace` fixture.
//!
//! No longer a home for elicit_doc-baseline comparison: cordial retired
//! its output-parity chase against `elicit_doc` once its own etiquettes
//! (e.g. the parent/Kind/native-source error architecture rules) started
//! finding real things elicit_doc never could -- there is no longer a
//! frozen reference to stay in lockstep with. See `docs/planning/
//! elicit-doc-parity.md`'s own status note for the historical record.

#[cfg(feature = "impl_coverage")]
mod minimal_fixture;
#[cfg(feature = "shadow")]
mod shadow_fixture;

#[cfg(feature = "impl_coverage")]
pub use minimal_fixture::{
    IMPL_GAPS_KEY_COLUMNS, filter_impl_gaps_by_crate, impl_gaps_open, normalize_elicit_impl_gaps,
    run_cordial_impl_coverage, seed_minimal_impl_fixture, write_minimal_rustdoc,
    write_minimal_rustdoc_file,
};
#[cfg(feature = "shadow")]
pub use shadow_fixture::{
    SHADOW_GAPS_KEY_COLUMNS, SHADOW_PAIR_KEY_COLUMNS, filter_shadow_gaps_by_target,
    filter_shadow_pair_by_item_path, run_cordial_shadow_coverage, seed_minimal_shadow_fixture,
    seed_shadow_dep_rustdoc, shadow_gaps_open,
};

use std::collections::HashMap;
use std::path::PathBuf;

use miette::{IntoDiagnostic, WrapErr};

/// Parsed CSV with header row.
#[derive(Debug, Clone)]
pub struct CsvTable {
    pub rows: Vec<HashMap<String, String>>,
}

impl CsvTable {
    pub fn parse(content: &str) -> miette::Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(content.as_bytes());
        let headers: Vec<String> = reader
            .headers()
            .into_diagnostic()
            .wrap_err("csv headers")?
            .iter()
            .map(str::to_string)
            .collect();
        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.into_diagnostic().wrap_err("csv record")?;
            rows.push(
                headers
                    .iter()
                    .zip(record.iter())
                    .map(|(header, value)| (header.clone(), value.to_string()))
                    .collect(),
            );
        }
        Ok(Self { rows })
    }

    pub fn open_rows<F>(&self, is_open: F) -> Vec<&HashMap<String, String>>
    where
        F: Fn(&HashMap<String, String>) -> bool,
    {
        self.rows.iter().filter(|row| is_open(row)).collect()
    }
}

fn parity_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity")
}

pub fn workspace_path(name: &str) -> PathBuf {
    parity_root().join("workspaces").join(name)
}

//! Loading exception rows and etiquette-indexed exception sets.

use std::collections::HashMap;

use tracing::instrument;

use crate::error::CordialResult;
use crate::store::StoreLayout;

use super::entry::ExceptionSet;
use super::json::parse_exception_file;

/// Load exceptions for one etiquette and crate.
///
/// Reads `{store}/exceptions/{etiquette}/{crate}.json` and, when present,
/// merges entries from the elicit_doc alias `{store}/quality/patches/{etiquette}/{crate}.json`.
#[instrument(level = "info", skip(store), fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_exceptions(
    store: &StoreLayout,
    etiquette_id: &str,
    crate_name: &str,
) -> CordialResult<ExceptionSet> {
    let file_name = format!("{crate_name}.json");
    let canonical = store.exceptions_dir().join(etiquette_id).join(&file_name);
    let alias = store
        .quality_patches_dir()
        .join(etiquette_id)
        .join(&file_name);

    let mut entries = Vec::new();
    if canonical.is_file() {
        entries.extend(parse_exception_file(&canonical)?);
    }
    if alias.is_file() {
        entries.extend(parse_exception_file(&alias)?);
    }
    Ok(ExceptionSet::from_entries(entries))
}

/// Load exception sets for all selected etiquettes.
#[instrument(level = "info", skip(store), fields(crate_name = crate_name), err(level = "warn"))]
pub fn load_exception_sets(
    store: &StoreLayout,
    etiquette_ids: &[&str],
    crate_name: &str,
) -> CordialResult<HashMap<String, ExceptionSet>> {
    let mut sets = HashMap::new();
    for etiquette_id in etiquette_ids {
        sets.insert(
            (*etiquette_id).to_string(),
            load_exceptions(store, etiquette_id, crate_name)?,
        );
    }
    Ok(sets)
}

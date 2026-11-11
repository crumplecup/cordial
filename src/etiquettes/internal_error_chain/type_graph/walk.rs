//! Type labels for the error type graph.

use std::collections::BTreeSet;

use tracing::instrument;

/// Last path segment of a type label (`io::IoSource` → `IoSource`).
#[instrument(level = "debug")]
pub(crate) fn last_ident(label: &str) -> &str {
    label.rsplit("::").next().unwrap_or(label)
}

/// Keep type-graph nodes that belong to a type (or variant of a type) that
/// implements `Error`.
#[instrument(level = "debug", skip(error_impls))]
pub(crate) fn type_path_is_error_related(type_path: &str, error_impls: &BTreeSet<String>) -> bool {
    type_path
        .split("::")
        .any(|segment| error_impls.contains(segment))
}

#[instrument(level = "trace", ret)]
pub(crate) fn is_foreign_type_label(label: &str) -> bool {
    [
        "std::",
        "serde_json::",
        "serde_yaml::",
        "syn::",
        "csv::",
        "cargo_metadata::",
        "reqwest::",
        "url::",
        "toml::",
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || (label.ends_with("Error") && label.contains("::"))
}

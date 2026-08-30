//! Minimal RFC 4180 CSV field escaping, shared by the hand-written report
//! writers under `etiquettes/*/reporter*.rs`. Not a general CSV library —
//! just enough that a comma, double quote, or newline inside a field
//! (e.g. a joined list of type names) can't silently shift every column
//! after it, the way `modularity.csv`'s `MODULARITY-TYPES-PER-FILE` rows
//! did before this existed: the row's `context` field joined multiple
//! type names with `, `, written unquoted, so a CSV reader split each
//! type name into its own (wrong) column.

use tracing::instrument;

/// Escape one CSV field per RFC 4180: wrap in double quotes if it
/// contains a comma, double quote, or newline, doubling any internal
/// double quote. Leaves a field with none of those characters untouched.
#[instrument(level = "trace", ret)]
pub(crate) fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

//! Label comparison shared by the source-shape lints.

use tracing::instrument;

#[instrument(level = "debug")]
pub(crate) fn type_labels_match(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!("::{b}")) || b.ends_with(&format!("::{a}"))
}

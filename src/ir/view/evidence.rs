//! Typed reads of optional and required evidence retained on the graph.

use super::IrView;
use crate::{CordialResult, NodeId};
use serde::de::DeserializeOwned;

#[tracing::instrument(level = "trace", skip(view, id), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn optional<T: DeserializeOwned>(
    view: &(impl IrView + ?Sized),
    id: NodeId,
    key: &str,
) -> CordialResult<Option<T>> {
    // Packed evidence goes straight to its typed consumer without retaining an
    // expanded JSON tree. Ordinary values are still borrowed during decoding.
    match view.node(id) {
        Some(node) => Ok(node.weight().read_attr(key)?),
        None => Ok(None),
    }
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "panics",
    feature = "antipatterns"
))]
#[tracing::instrument(level = "trace", skip(view), err(level = "warn"))]
pub(super) fn required<T: DeserializeOwned>(
    view: &(impl IrView + ?Sized),
    key: &str,
    description: &'static str,
) -> CordialResult<T> {
    optional(view, view.root()?, key)?
        .ok_or_else(|| crate::CordialError::workspace_source_facts_missing(description))
}

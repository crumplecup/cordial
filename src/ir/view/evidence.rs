//! Typed reads of optional and required evidence retained on the graph.

use super::IrView;
use crate::{CordialResult, NodeId};
use serde::de::DeserializeOwned;

#[tracing::instrument(level = "trace", skip(view, id), err(level = "warn"))]
pub(super) fn optional<T: DeserializeOwned>(
    view: &(impl IrView + ?Sized),
    id: NodeId,
    key: &str,
) -> CordialResult<Option<T>> {
    Ok(view
        .node(id)
        .and_then(|node| node.attr(key).cloned())
        .map(serde_json::from_value)
        .transpose()?)
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir"
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

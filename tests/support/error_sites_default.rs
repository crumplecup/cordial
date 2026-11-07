//! The default error-site hooks for the shared site materializer.
//!
//! Kept apart from `error_sites_projection.rs` so a test that never uses the
//! default hooks does not compile an unused wrapper.

use crate::projection::materialized_with;
use cordial::CrateIr;
use miette::Result;
use std::path::Path;

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn materialized(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
    materialized_with(graph, root, &["error-site-inventory"], "error_site_kind")
}

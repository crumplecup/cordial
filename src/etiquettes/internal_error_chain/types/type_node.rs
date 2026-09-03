use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{InternalErrorNodeClass, InternalErrorTypeProbeId};

/// One row in the static error type graph inventory.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InternalErrorTypeNode {
    /// Cargo package name.
    crate_name: String,
    /// Qualified type path.
    type_path: String,
    /// Role of this type in the internal error graph.
    #[getter(copy)]
    node_class: InternalErrorNodeClass,
    /// Probe that classified this type node.
    #[getter(copy)]
    probe_id: InternalErrorTypeProbeId,
    /// Type this error wraps, when it has a source.
    source_target: Option<String>,
    /// Whether following `source()` reaches a foreign error.
    #[getter(copy)]
    reaches_foreign: bool,
    /// How many `source()` hops were recorded.
    #[getter(copy)]
    chain_depth: u32,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Source snippet captured at the site.
    snippet: String,
}

impl InternalErrorTypeNode {
    /// Start a builder for this value.
    pub fn builder() -> InternalErrorTypeNodeBuilder {
        InternalErrorTypeNodeBuilder::default()
    }

    pub(crate) fn strip_file_prefix(&mut self, root: &std::path::Path) {
        if let Ok(rel) = self.file.strip_prefix(root) {
            self.file = rel.to_path_buf();
        }
    }
}

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{InternalErrorNodeClass, InternalErrorTypeProbeId};

/// One row in the static error type graph inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorTypeNode {
    /// Cargo package name.
    pub crate_name: String,
    /// Qualified type path.
    pub type_path: String,
    /// Role of this type in the internal error graph.
    pub node_class: InternalErrorNodeClass,
    /// Probe that classified this type node.
    pub probe_id: InternalErrorTypeProbeId,
    /// Type this error wraps, when it has a source.
    pub source_target: Option<String>,
    /// Whether following `source()` reaches a foreign error.
    pub reaches_foreign: bool,
    /// How many `source()` hops were recorded.
    pub chain_depth: u32,
    /// Source file path, usually crate-relative.
    pub file: PathBuf,
    /// Source line number (1-based), when known.
    pub line: u32,
    /// Source snippet captured at the site.
    pub snippet: String,
}

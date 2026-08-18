use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{InternalErrorNodeClass, InternalErrorTypeProbeId};

/// One row in the static error type graph inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorTypeNode {
    pub crate_name: String,
    pub type_path: String,
    pub node_class: InternalErrorNodeClass,
    pub probe_id: InternalErrorTypeProbeId,
    pub source_target: Option<String>,
    pub reaches_foreign: bool,
    pub chain_depth: u32,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}

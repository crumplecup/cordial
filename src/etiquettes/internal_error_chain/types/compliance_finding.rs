use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::InternalErrorComplianceId;

/// One non-compliant error-handling site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorComplianceFinding {
    pub crate_name: String,
    pub rule_id: InternalErrorComplianceId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
    pub foreign_error_type: Option<String>,
    pub internal_constructor: Option<String>,
}

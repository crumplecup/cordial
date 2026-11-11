//! Merge database-owned antipattern facts, contract bounds and workspace versions.

use std::collections::HashMap;
use std::path::Path;

use crate::error::CordialResult;

use super::contract_bounds::{ContractClause, fetch_contract_records};
use super::types::AntipatternSiteRecord;
use super::version_in_member::scan_workspace_version_in_member;
use tracing::instrument;

/// Workspace evidence shared by every member's antipattern inventory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_new::new)]
pub struct AntipatternWorkspaceFacts {
    registry: Vec<super::contract_bounds::ContractRecordDump>,
    version_by_crate: HashMap<String, Vec<AntipatternSiteRecord>>,
}

impl AntipatternWorkspaceFacts {
    #[instrument(level = "debug", err(level = "warn"))]
    /// Load registry and workspace-version evidence shared by every member.
    pub fn load(workspace_root: &Path, store_root: &Path) -> CordialResult<Self> {
        Ok(Self {
            registry: fetch_contract_records(workspace_root, store_root),
            version_by_crate: scan_workspace_version_in_member(workspace_root)?,
        })
    }
}

impl AntipatternWorkspaceFacts {
    #[instrument(level = "trace", skip(self, clauses))]
    pub(super) fn assess_contracts(
        &self,
        clauses: &[ContractClause],
    ) -> Vec<AntipatternSiteRecord> {
        super::contract_bounds::assess_contracts(clauses, &self.registry)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn version_records(&self, crate_name: &str) -> &[AntipatternSiteRecord] {
        self.version_by_crate
            .get(crate_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

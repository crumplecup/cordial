//! Merge per-crate antipattern scans (syn, contract bounds, version-in-member).

use std::collections::HashMap;
use std::path::Path;

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::error::CordialResult;
use crate::loader::SourceLoadView;

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
    #[instrument(level = "debug")]
    pub(crate) fn load(workspace_root: &Path, store_root: &Path) -> CordialResult<Self> {
        Ok(Self {
            registry: fetch_contract_records(workspace_root, store_root),
            version_by_crate: scan_workspace_version_in_member(workspace_root)?,
        })
    }
}

/// Scan one crate for antipatterns.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_antipatterns(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    let facts = AntipatternWorkspaceFacts::load(workspace_root, store_root)?;
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        &facts,
        StaticRefStrategy::default(),
        ErrorFieldStrategy::default(),
    )
}

/// Scan one crate for antipatterns using a static-reference remediation strategy.
#[instrument(level = "debug", skip(static_ref_strategy))]
pub fn scan_crate_antipatterns_with_static_ref_strategy(
    crate_root: &Path,
    crate_name: &str,
    workspace_root: &Path,
    store_root: &Path,
    static_ref_strategy: StaticRefStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    let facts = AntipatternWorkspaceFacts::load(workspace_root, store_root)?;
    scan_shared_crate_antipatterns(
        &source,
        crate_root,
        crate_name,
        &facts,
        static_ref_strategy,
        ErrorFieldStrategy::default(),
    )
}

#[instrument(
    level = "debug",
    skip(source, facts, static_ref_strategy, error_field_strategy),
    err(level = "warn")
)]
pub(super) fn scan_shared_crate_antipatterns(
    source: &SourceLoadView,
    crate_root: &Path,
    crate_name: &str,
    facts: &AntipatternWorkspaceFacts,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let catalog = super::catalog::AntipatternCatalog::collect(source, crate_root, crate_name)?;
    catalog.assess(crate_name, facts, static_ref_strategy, error_field_strategy)
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

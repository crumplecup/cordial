//! Stdio policy applied to shared loader evidence.
use super::types::{PrintRuleId, PrintSiteRecord};
use crate::config::TracingStdioPolicy;
use crate::error::CordialResult;
use crate::loader::SourceLoadView;
use std::path::Path;
use tracing::instrument;

/// Scan `src/` and `tests/` for leftover std print macros.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_tracing_print(
    root: &Path,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    findings_from_inventory(
        &crate::ir::SourceObservability::collect(&source, root)?,
        policy,
    )
}

#[instrument(level = "debug", skip(inventory, policy), err(level = "warn"))]
pub(super) fn findings_from_inventory(
    inventory: &crate::ir::SourceObservability,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let mut findings = Vec::new();
    for file in inventory.files() {
        if policy.skips_file(file.snapshot().path(), inventory.crate_root()) {
            continue;
        }
        findings.extend(file_findings(file, policy)?);
    }
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.snippet().cmp(b.snippet()))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(file, policy), err(level = "warn"))]
fn file_findings(
    file: &crate::ir::ObservabilityFile,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let mut findings = Vec::new();
    if !file.syntax_valid() {
        return Err(crate::CordialError::workspace_source_facts_missing(
            "valid stdio source syntax",
        ));
    }
    for site in file.prints() {
        if policy.skip_cargo_protocol() && site.cargo_protocol() {
            continue;
        }
        let enabled = match site.rule() {
            PrintRuleId::Println => policy.println(),
            PrintRuleId::Eprintln => policy.eprintln(),
            PrintRuleId::Print => policy.print(),
            PrintRuleId::Eprint => policy.eprint(),
            PrintRuleId::Dbg => policy.dbg(),
        };
        if !enabled {
            continue;
        }
        findings.push(
            PrintSiteRecord::builder()
                .rule_id(site.rule())
                .context(site.context().clone())
                .file(file.relative().clone())
                .line(site.line())
                .snippet(site.rule().snippet().to_string())
                .build()?,
        );
    }
    Ok(findings)
}

/// Scan one Rust source fixture through the canonical source database.
#[instrument(level = "debug", skip(source, policy, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let inventory =
        crate::ir::SourceObservability::from_fixture(source, file, tree_root, crate_root)?;
    // Explicit single-file scans do not apply discovery exclusions and retain
    // source order, matching the fixture API's existing contract.
    let mut findings = Vec::new();
    for file in inventory.files() {
        findings.extend(file_findings(file, policy)?);
    }
    Ok(findings)
}

//! Internal error-chain reports read from the database-backed inventories.
use cordial::{
    ErrorFieldStrategy, InternalErrorChainScanReport, InternalErrorChainThresholds,
    InternalErrorComplianceReport, IrView,
};
use miette::{IntoDiagnostic, Result, WrapErr};
use std::path::Path;

#[path = "source_projection.rs"]
mod source_projection;

/// The combined report the loader's inventories give for the crate at `root`.
/// A fixture without a manifest gets a minimal one named `crate_name`.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn internal_report(root: &Path, crate_name: &str) -> Result<InternalErrorChainScanReport> {
    write_manifest_if_absent(root, crate_name)?;
    report_for(root, crate_name)
}

fn write_manifest_if_absent(root: &Path, crate_name: &str) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        return Ok(());
    }
    std::fs::write(
        manifest,
        format!(
            "[package]\nname = '{crate_name}'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n"
        ),
    )
    .into_diagnostic()
}

fn report_for(root: &Path, crate_name: &str) -> Result<InternalErrorChainScanReport> {
    let target = cordial::discover_crate_targets(root, &cordial::RunAll)
        .into_diagnostic()?
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("no crate target under {}", root.display()))?;
    let ir = source_projection::baseline(root, &target)?;
    let type_graph = ir
        .source_error_types()
        .into_diagnostic()?
        .report(crate_name, false)
        .into_diagnostic()
        .wrap_err("type graph")?;
    let signatures = ir
        .source_error_signatures()
        .into_diagnostic()?
        .assess(crate_name, false)
        .into_diagnostic()
        .wrap_err("signatures")?;
    let mut findings = ir
        .source_error_compliance()
        .into_diagnostic()?
        .findings(false);
    findings.extend(signatures.findings().iter().cloned());
    findings.extend(
        ir.source_error_architecture()
            .into_diagnostic()?
            .assess(
                InternalErrorChainThresholds::default().kind_fanout_max_call_sites(),
                ErrorFieldStrategy::default(),
                false,
            )
            .into_diagnostic()
            .wrap_err("architecture")?,
    );
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().to_string().cmp(&b.rule_id().to_string()))
    });
    Ok(InternalErrorChainScanReport::new(
        crate_name.to_string(),
        type_graph,
        InternalErrorComplianceReport::new(crate_name.to_string(), findings),
        signatures.checks().clone(),
    ))
}

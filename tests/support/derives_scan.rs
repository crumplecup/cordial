//! Scans a Rust source snippet with the derives lint and returns the records it raises.

use cordial::{DeriveSiteRecord, DerivesThresholds, PathInclusionFacts, scan_derives_rust_source};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

pub fn scan_findings(
    source: &str,
    thresholds: DerivesThresholds,
) -> miette::Result<Vec<DeriveSiteRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write sample")?;
    scan_derives_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        thresholds,
        &PathInclusionFacts::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")
}

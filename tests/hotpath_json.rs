//! Published Hotpath JSON remains consumable without any fork types.
#![cfg(feature = "hotpath")]
use miette::{IntoDiagnostic, Result};

#[cfg(not(feature = "hotpath-alloc"))]
#[test]
fn timing_only_capture_has_no_allocation_section() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let path = directory.path().join("timing.json");
    let guard = hotpath::HotpathGuardBuilder::new("timing")
        .format(hotpath::Format::JsonPretty)
        .output_path(&path)
        .build();
    drop(guard);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).into_diagnostic()?).into_diagnostic()?;
    assert!(report["functions_alloc"].is_null());
    assert!(report.get("allocation").is_none());
    Ok(())
}

#[test]
fn allocation_counts_do_not_become_byte_measurements() -> Result<()> {
    let report: serde_json::Value = serde_json::from_str(r#"{"functions_alloc":{"profiling_mode":"alloc-count","data":[{"name":"fixture","calls":1,"total":"123 B"}]}}"#).into_diagnostic()?;
    assert!(
        cordial::perf::ProfileUsage::from_report(&report)
            .allocations()
            .is_empty()
    );
    Ok(())
}

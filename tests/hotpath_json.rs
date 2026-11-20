//! Machine-readable allocation states and integer precision, without workloads.
#![cfg(feature = "hotpath")]

use miette::{IntoDiagnostic, Result};

#[test]
fn exact_allocation_fields_preserve_large_integers_and_missing_values() -> Result<()> {
    let mut report = hotpath::json::AllocationReport::disabled();
    assert_eq!(
        report.functions_status,
        hotpath::json::AllocationStatus::Disabled
    );
    assert!(report.functions.is_none());
    report.functions_status = hotpath::json::AllocationStatus::Available;
    report.accounting = Some(hotpath::json::AllocationAccounting::Exclusive);
    report.functions = Some(vec![hotpath::json::ExactFunctionAllocation {
        id: 7,
        name: "numeric fixture".to_owned(),
        calls: (1_u64 << 53) + 1,
        bytes: Some(u64::MAX - 1),
        count: None,
    }]);
    let json = serde_json::to_string(&report).into_diagnostic()?;
    let decoded: hotpath::json::AllocationReport = serde_json::from_str(&json).into_diagnostic()?;
    assert_eq!(decoded, report);
    assert!(json.contains("18446744073709551614"));
    assert!(json.contains("9007199254740993"));
    assert!(json.contains("\"count\":null"));
    Ok(())
}

#[cfg(not(feature = "hotpath-alloc"))]
#[test]
fn timing_only_capture_reports_allocation_disabled() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let path = directory.path().join("disabled.json");
    let guard = hotpath::HotpathGuardBuilder::new("disabled")
        .format(hotpath::Format::JsonPretty)
        .output_path(&path)
        .build();
    drop(guard);
    let report: hotpath::json::JsonReport =
        serde_json::from_str(&std::fs::read_to_string(path).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(
        report.allocation,
        Some(hotpath::json::AllocationReport::disabled())
    );
    Ok(())
}

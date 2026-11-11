//! Error-type antipatterns: `Box<dyn Error>` and `String` error results.

use cordial::{AntipatternRuleId, scan_antipatterns_rust_source};
use miette::IntoDiagnostic;
use std::path::Path;

#[path = "support/antipatterns_fixture.rs"]
mod antipatterns_fixture;

#[path = "support/antipatterns_scan.rs"]
mod antipatterns_scan;

use antipatterns_scan::scan_fixture;

#[test]
fn box_dyn_error_sites_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("box_dyn_error.rs")?;
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule_id() == AntipatternRuleId::BoxDynError001)
            .count(),
        4
    );
    Ok(())
}

#[test]
fn box_dyn_error_context_includes_enclosing_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("box_dyn_error.rs")?;
    let accepts = findings
        .iter()
        .find(|f| f.context().contains("accepts"))
        .ok_or_else(|| miette::miette!("accepts finding"))?;
    assert_eq!(accepts.rule_id(), AntipatternRuleId::BoxDynError001);
    assert!(accepts.snippet().contains("Box<dyn Error>"));
    Ok(())
}

#[test]
fn non_box_dyn_error_types_are_ignored() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("box_dyn_error.rs")?;
    assert!(
        !findings
            .iter()
            .any(|f| f.snippet().contains("Display") || f.snippet().contains("&'static"))
    );
    Ok(())
}

#[test]
fn string_error_result_types_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("string_error.rs")?;
    let string_errors: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StringError001)
        .collect();
    assert_eq!(string_errors.len(), 4, "{string_errors:?}");
    assert!(
        string_errors
            .iter()
            .any(|f| f.context().contains("returns_string"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context().contains("returns_str"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context().contains("returns_std_string"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context().contains("StringResult"))
    );
    assert!(
        !string_errors
            .iter()
            .any(|f| f.context().contains("ok_is_string") || f.context().contains("typed_error"))
    );
    Ok(())
}

#[test]
fn aliased_one_arg_result_is_not_a_string_error() -> miette::Result<()> {
    cordial::init_tracing();
    let src_root = Path::new(".");
    let file = src_root.join("aliased.rs");
    let findings = scan_antipatterns_rust_source(
        "fn io_ok() -> std::io::Result<u8> { Ok(0) }\n\
         fn report() -> miette::Result<String> { Ok(String::new()) }\n",
        &file,
        src_root,
        src_root,
    )
    .into_diagnostic()?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule_id() != AntipatternRuleId::StringError001),
        "one-arg Result aliases are not Result<_, String>: {findings:?}"
    );
    Ok(())
}

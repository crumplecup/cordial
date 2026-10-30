#![cfg(feature = "panics")]
//! Result evidence is collected from the shared parse and survives IR persistence.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::fs;

#[test]
fn standard_contracts_and_located_calls_come_from_production_ir() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"Ok(8)\n");
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let source = fs::read_to_string(fixture.path().join("src/main.rs")).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    for name in ["operation", "leaf"] {
        let facts = function_facts(&ir, name)?;
        let contract = facts
            .result_contract()
            .as_ref()
            .ok_or_else(|| miette::miette!("missing {name} contract"))?;
        assert_eq!(contract.success_syntax(), "u32");
        assert_eq!(
            contract.error_identity().as_deref(),
            Some("::std::io::Error")
        );
    }
    let facts = function_facts(&ir, "leaf")?;
    let site = &facts.unwraps()[0];
    assert_eq!(
        &source[site.range().start()..site.range().end()],
        "operation(fail).unwrap()"
    );
    assert_eq!(
        &source[site.receiver().start()..site.receiver().end()],
        "operation(fail)"
    );
    assert!(
        facts
            .calls()
            .iter()
            .any(|call| call.range() == site.receiver()
                && call.local_name().as_deref() == Some("operation"))
    );
    Ok(())
}

#[test]
fn unresolved_outputs_and_shadowed_calls_do_not_acquire_proof() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_unresolved.rs"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    assert!(function_facts(&ir, "relative")?.result_contract().is_none());
    assert!(function_facts(&ir, "alias")?.result_contract().is_none());
    assert!(
        function_facts(&ir, "unknown_error")?
            .result_contract()
            .as_ref()
            .is_some_and(|contract| contract.error_identity().is_none())
    );
    assert!(
        function_facts(&ir, "shadowed")?.calls()[0]
            .local_name()
            .is_none()
    );
    assert!(function_facts(&ir, "nested")?.unwraps().is_empty());
    Ok(())
}

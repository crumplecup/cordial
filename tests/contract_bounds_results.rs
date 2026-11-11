//! Bare `result` in Verus contracts and raw Kani assertions: what is trivial and what carries content.

#![cfg(feature = "antipatterns")]

use cordial::{
    AntipatternRuleId, scan_kani_contract_bounds_source, scan_verus_contract_bounds_source,
};
use miette::{IntoDiagnostic, WrapErr};

#[path = "support/contract_bounds_fixtures.rs"]
mod contract_bounds_fixtures;

use contract_bounds_fixtures::fixtures_root;

#[test]
fn verus_bare_result_is_none_is_never_flagged_but_is_some_content_is() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: (Option<i32>, Option<i32>, Option<i32>))
    ensures
        result.0 == Some(1),
        result.1 == Some(2),
        result.2 is None,
{
    (Some(1), Some(2), None)
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_is_none.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus is_none")?;

    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|finding| {
        finding.snippet().contains("Some")
            && finding.snippet().contains('1')
            && finding.snippet().contains("result . 0")
    }));
    assert!(findings.iter().any(|finding| {
        finding.snippet().contains("Some")
            && finding.snippet().contains('2')
            && finding.snippet().contains("result . 1")
    }));
    assert!(
        !findings
            .iter()
            .any(|finding| finding.snippet().contains("is None"))
    );
    Ok(())
}

#[test]
fn verus_bare_result_tuple_projections_are_never_flagged_but_comparisons_are() -> miette::Result<()>
{
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something(initial: i32, updated: i32) -> (result: (bool, bool, i32))
    ensures
        result.0,
        !result.1,
        result.2 == updated,
{
    (true, false, updated)
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_tuple.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus tuple")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].snippet().contains("result . 2 == updated"));
    Ok(())
}

#[test]
fn kani_raw_assert_eq_is_flagged_as_the_synthesized_equality() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_RAW_EQ_SRC, {
        #[kani::proof]
        fn verify_raw_eq() {
            let value: i32 = kani::any();
            assert_eq!(value, value + 0, "raw equation via assert_eq");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_assert_eq.rs");
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan raw assert_eq kani")?;

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].rule_id(),
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].snippet().contains("value == value + 0"));
    Ok(())
}

#[test]
fn kani_raw_assume_is_now_flagged_when_unregistered() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_RAW_ASSUME_SRC, {
        #[kani::proof]
        fn verify_raw_assume() {
            let value: i32 = kani::any();
            kani::assume(value != 42);
            assert!(true);
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_assume.rs");
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan raw assume kani")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].snippet().contains("value != 42"));
    Ok(())
}

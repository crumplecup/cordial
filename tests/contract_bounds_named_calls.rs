//! Named calls in contracts: matched against registered functions and types, flagged when unregistered.

#![cfg(feature = "antipatterns")]

use cordial::{
    AntipatternRuleId, scan_creusot_contract_bounds_source, scan_kani_contract_bounds_source,
    scan_verus_contract_bounds_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/contract_bounds_fixtures.rs"]
mod contract_bounds_fixtures;

use contract_bounds_fixtures::fixtures_root;

#[path = "support/contract_bounds_dump.rs"]
mod contract_bounds_dump;

#[path = "support/contract_bounds_kani_record.rs"]
mod contract_bounds_kani_record;

#[path = "support/contract_bounds_logic_record.rs"]
mod contract_bounds_logic_record;

use contract_bounds_kani_record::kani_type_record;
use contract_bounds_logic_record::logic_fn_record;

fn fixture(name: &str) -> miette::Result<String> {
    fs::read_to_string(fixtures_root().join(name))
        .into_diagnostic()
        .wrap_err_with(|| format!("read fixture {name}"))
}

// ---------------------------------------------------------------------
// Coverage beyond the shape matrix: fixture-driven, multi-clause, and
// crate/session-level tests whose assertions inspect specific finding
// content or use a different entry point (`scan_crate_contract_bounds`,
// `SessionBuilder`) rather than a single source string's flagged/silent
// outcome. These stay as dedicated tests rather than table rows.
// ---------------------------------------------------------------------

#[test]
fn creusot_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("creusot", "ensures", "char_roundtrips")?];
    let findings = scan_creusot_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan creusot fixture")?;

    assert_eq!(findings.len(), 2);
    assert!(
        findings
            .iter()
            .any(|finding| finding.context().contains("verify_something_raw")
                && finding.snippet().contains("result >= 0"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.context().contains("verify_pearlite_only")
                && finding.snippet().contains("0xD7FF"))
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context().contains("verify_char_roundtrip"))
    );
    Ok(())
}

#[test]
fn creusot_named_call_to_an_unregistered_fn_name_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let findings =
        scan_creusot_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan creusot fixture")?;

    assert_eq!(findings.len(), 3);
    assert!(findings.iter().any(|finding| {
        finding.context().contains("verify_char_roundtrip")
            && finding.snippet().contains("char_roundtrips")
    }));
    Ok(())
}

#[test]
fn creusot_trivial_requires_true_is_never_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let findings =
        scan_creusot_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan creusot fixture")?;

    assert!(!findings.iter().any(|finding| finding.snippet() == "true"));
    Ok(())
}

#[test]
fn verus_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_verus.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("verus", "ensures", "char_roundtrips")?];
    let findings = scan_verus_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan verus fixture")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context().contains("verify_something_raw"));
    assert!(findings[0].snippet().contains("value >= 0"));
    Ok(())
}

#[test]
fn verus_assume_specification_semicolon_terminator_does_not_leak_into_the_next_item()
-> miette::Result<()> {
    cordial::init_tracing();
    // `assume_specification` (and any other body-less Verus declaration)
    // ends its `ensures`/`requires` list with a bare `;`, not a brace
    // group. Without a `;` stop case in the clause-list scanner, the scan
    // runs past the semicolon and swallows the next item's doc-comment
    // attributes and signature as if they were more of the same clause
    // list, manufacturing a phantom finding out of unrelated tokens.
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub assume_specification<'a, B: ToOwned + ?Sized> [Cow::<'a, B>::into_owned] (cow: Cow<'a, B>) -> (result: <B as ToOwned>::Owned)
    ensures
        cow_into_owned_preserves_variant_value(cow, result),
;

/// Unrelated doc comment on the next item -- must never be folded into
/// the `assume_specification` above's clause list.
pub fn verify_something(value: i32) -> (result: i32)
    ensures
        result == value,
{
    value
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_assume_spec.rs");
    let registry = vec![logic_fn_record(
        "verus",
        "ensures",
        "cow_into_owned_preserves_variant_value",
    )?];
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan inline verus assume_specification")?;

    // `cow_into_owned_preserves_variant_value(cow, result)` is a real
    // registered call, silenced. The only real finding left is
    // `verify_something`'s own `result == value` -- never a phantom
    // "clause" made of the semicolon, doc comment, and next signature.
    assert_eq!(findings.len(), 1);
    assert!(findings[0].context().contains("verify_something"));
    assert!(findings[0].snippet().contains("result == value"));
    Ok(())
}

#[test]
fn kani_named_call_matching_a_registered_type_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_kani.rs";
    let src_root = fixtures_root();
    let registry = vec![
        kani_type_record("requires", "fixture::NonNegative")?,
        kani_type_record("ensures", "fixture::NonNegative")?,
    ];
    let findings = scan_kani_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan kani fixture")?;

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].rule_id(),
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].context().contains("verify_raw"));
    assert!(findings[0].snippet().contains("value < 100"));
    Ok(())
}

#[test]
fn kani_named_call_to_an_unregistered_type_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_kani.rs";
    let src_root = fixtures_root();
    let findings =
        scan_kani_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan kani fixture")?;

    assert_eq!(findings.len(), 3);
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet().contains("NonNegative :: requires"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet().contains("NonNegative :: ensures"))
    );
    Ok(())
}

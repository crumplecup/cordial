#![cfg(feature = "antipatterns")]

use std::fs;
use std::path::PathBuf;

use cordial::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, ContractRecordDump, RunAll, Session, SessionBuilder,
    scan_crate_contract_bounds, scan_creusot_contract_bounds_source,
    scan_kani_contract_bounds_source, scan_verus_contract_bounds_source,
};

use miette::{IntoDiagnostic, WrapErr};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quality/antipatterns")
}

fn fixture(name: &str) -> miette::Result<String> {
    fs::read_to_string(fixtures_root().join(name))
        .into_diagnostic()
        .wrap_err_with(|| format!("read fixture {name}"))
}

fn logic_fn_record(verifier: &str, kind: &str, fn_name: &str) -> ContractRecordDump {
    ContractRecordDump {
        evidence: format!("fixture::{fn_name}"),
        verifier: verifier.to_string(),
        kind: kind.to_string(),
        fragment: format!("#[logic(open)]\nfn {fn_name}(c: char) -> bool {{\n    true\n}}"),
    }
}

fn kani_type_record(kind: &str, evidence: &str) -> ContractRecordDump {
    ContractRecordDump {
        evidence: evidence.to_string(),
        verifier: "kani".to_string(),
        kind: kind.to_string(),
        fragment: "value >= 0".to_string(),
    }
}

#[test]
fn creusot_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("creusot", "ensures", "char_roundtrips")];
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
            .any(|finding| finding.context.contains("verify_something_raw")
                && finding.snippet.contains("result >= 0"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.context.contains("verify_pearlite_only")
                && finding.snippet.contains("0xD7FF"))
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context.contains("verify_char_roundtrip"))
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
        finding.context.contains("verify_char_roundtrip")
            && finding.snippet.contains("char_roundtrips")
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

    assert!(!findings.iter().any(|finding| finding.snippet == "true"));
    Ok(())
}

#[test]
fn creusot_trivial_bare_result_is_never_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    creusot, VERIFY_SOMETHING_SRC, {
        #[trusted]
        #[ensures(result)]
        fn verify_something() -> bool {
            match 1 {
                1 => true,
                _ => false,
            }
        }
    }
}
"#;
    let path = fixtures_root().join("inline_creusot.rs");
    let findings = scan_creusot_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan inline creusot")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn verus_trivial_bare_result_is_never_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        result,
{
    true
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan inline verus")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn verus_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_verus.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("verus", "ensures", "char_roundtrips")];
    let findings = scan_verus_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan verus fixture")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context.contains("verify_something_raw"));
    assert!(findings[0].snippet.contains("value >= 0"));
    Ok(())
}

#[test]
fn verus_named_call_with_final_argument_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

pub struct VerusCellModel {
    value: i32,
}

verus! {

impl VerusCellModel {
    pub fn set(&mut self, new_value: i32)
        ensures
            write_stores_new_value(new_value as int, final(self).value as int),
    {
        self.value = new_value;
    }
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_final_call.rs");
    let registry = vec![logic_fn_record(
        "verus",
        "ensures",
        "write_stores_new_value",
    )];
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan inline verus final-arg call")?;

    assert!(findings.is_empty());
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
    )];
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
    assert!(findings[0].context.contains("verify_something"));
    assert!(findings[0].snippet.contains("result == value"));
    Ok(())
}

#[test]
fn kani_named_call_matching_a_registered_type_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_kani.rs";
    let src_root = fixtures_root();
    let registry = vec![
        kani_type_record("requires", "fixture::NonNegative"),
        kani_type_record("ensures", "fixture::NonNegative"),
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
        findings[0].rule_id,
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].context.contains("verify_raw"));
    assert!(findings[0].snippet.contains("value < 100"));
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
            .any(|finding| finding.snippet.contains("NonNegative :: requires"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet.contains("NonNegative :: ensures"))
    );
    Ok(())
}

#[test]
fn kani_type_suffix_match_handles_an_abbreviated_call_site_name() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_ABBREVIATED_SRC, {
        #[kani::proof]
        fn verify_abbreviated() {
            let value: i32 = kani::any();
            assert!(RustStdStandard::<i32>::ensures(value), "message");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_abbreviated.rs");
    let registry = vec![ContractRecordDump {
        evidence: "amenable_std::rust_std::RustStdStandard<i32>".to_string(),
        verifier: "kani".to_string(),
        kind: "ensures".to_string(),
        fragment: "value >= 0".to_string(),
    }];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan abbreviated kani")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn kani_type_suffix_match_handles_a_nested_generic_evidence_type() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_NESTED_GENERIC_SRC, {
        #[kani::proof]
        fn verify_nested_generic() {
            let cell = std::cell::Cell::new(0i32);
            assert!(RustStdStandard::<Cell<i32>>::ensures((cell.get(), 0)), "message");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_nested_generic.rs");
    let registry = vec![ContractRecordDump {
        evidence: "amenable_std::rust_std::RustStdStandard<Cell<i32>>".to_string(),
        verifier: "kani".to_string(),
        kind: "ensures".to_string(),
        fragment: "actual == expected".to_string(),
    }];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan nested generic kani")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn kani_negated_call_to_a_registered_type_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_REJECTS_ZERO_SRC, {
        #[kani::proof]
        fn verify_rejects_zero() {
            let value: i32 = kani::any();
            assert!(!fixture::NonNegative::ensures(value), "message");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_negated.rs");
    let registry = vec![kani_type_record("ensures", "fixture::NonNegative")];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan negated kani")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn kani_double_negated_call_is_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_DOUBLE_NEGATED_SRC, {
        #[kani::proof]
        fn verify_double_negated() {
            let value: i32 = kani::any();
            assert!(!!fixture::NonNegative::ensures(value), "message");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_double_negated.rs");
    let registry = vec![kani_type_record("ensures", "fixture::NonNegative")];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan double negated kani")?;

    assert_eq!(findings.len(), 1);
    Ok(())
}

#[test]
fn kani_fully_qualified_call_matching_a_registered_type_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    // `<Type as Trait>::method(...)` -- the disambiguating form this
    // workspace's own `CONTRACT_BOUND_NAMING_WORKFLOW.md` documents as
    // the real fix for a competing-impl ambiguity, not just a stylistic
    // variant of `Type::ensures(...)`. `syn` represents this as a `Path`
    // with `qself: Some(..)`, not as extra leading path segments -- a
    // real, previously-unrecognized shape distinct from the plain
    // `<TypePath>::ensures(...)` case the other Kani tests here cover.
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_QUALIFIED_SRC, {
        #[kani::proof]
        fn verify_qualified() {
            let value: i32 = kani::any();
            assert!(
                <fixture::NonNegative as Ensures<KaniVerifier>>::ensures(value),
                "message"
            );
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_qualified.rs");
    let registry = vec![kani_type_record("ensures", "fixture::NonNegative")];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan qualified kani")?;

    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn kani_assert_eq_call_shape_is_never_recognized_as_a_named_call() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_STILL_RAW_SRC, {
        #[kani::proof]
        fn verify_still_raw() {
            let value: i32 = kani::any();
            assert_eq!(fixture::NonNegative::round_trip(value), value);
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_assert_eq_call.rs");
    let registry = vec![kani_type_record("ensures", "fixture::NonNegative")];
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan assert_eq call kani")?;

    assert_eq!(findings.len(), 1);
    Ok(())
}

#[test]
fn verus_negated_bare_result_is_never_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        !result,
{
    false
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_negated.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan negated verus")?;

    assert!(findings.is_empty());
    Ok(())
}

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
        finding.snippet.contains("Some")
            && finding.snippet.contains('1')
            && finding.snippet.contains("result . 0")
    }));
    assert!(findings.iter().any(|finding| {
        finding.snippet.contains("Some")
            && finding.snippet.contains('2')
            && finding.snippet.contains("result . 1")
    }));
    assert!(
        !findings
            .iter()
            .any(|finding| finding.snippet.contains("is None"))
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
    assert!(findings[0].snippet.contains("result . 2 == updated"));
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
        findings[0].rule_id,
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].snippet.contains("value == value + 0"));
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
    assert!(findings[0].snippet.contains("value != 42"));
    Ok(())
}

#[test]
fn kani_gallery_directory_is_pruned_from_the_crate_walk() -> miette::Result<()> {
    cordial::init_tracing();
    let crate_root = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let src_root = crate_root.path().join("src");
    let gallery_root = src_root.join("gallery");
    fs::create_dir_all(&gallery_root)
        .into_diagnostic()
        .wrap_err("gallery dir")?;

    fs::write(
        src_root.join("lib.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_PRODUCTION_SRC, {
        #[kani::proof]
        fn verify_production() {
            let value: i32 = kani::any();
            assert!(value < 100, "raw bound");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write production harness")?;

    fs::write(
        gallery_root.join("experiment.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_EXPERIMENT_SRC, {
        #[kani::proof]
        fn verify_gallery_experiment_times_out() {
            let value: i32 = kani::any();
            assert!(value < 100, "raw bound, same shape as the production one");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write gallery harness")?;

    let findings = scan_crate_contract_bounds(crate_root.path(), "amenable_kani", &[])
        .into_diagnostic()
        .wrap_err("scan crate")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context.contains("verify_production"));
    assert!(!findings[0].file.starts_with("src/gallery"));
    Ok(())
}

#[test]
fn checklist_groups_same_shape_findings_into_a_duplicate_cluster() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = workspace.path().join("amenable_kani");
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;

    fs::write(
        workspace.path().join("Cargo.toml"),
        r#"[workspace]
members = ["amenable_kani"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
"#,
    )
    .into_diagnostic()
    .wrap_err("write workspace manifest")?;

    fs::write(
        crate_root.join("Cargo.toml"),
        r#"[package]
name = "amenable_kani"
version.workspace = true
edition.workspace = true
"#,
    )
    .into_diagnostic()
    .wrap_err("write crate manifest")?;

    fs::write(
        src_root.join("lib.rs"),
        "mod alloc_collections;\nmod alloc_ffi;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib.rs")?;

    fs::write(
        src_root.join("alloc_collections.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_MAP_SRC, {
        #[kani::proof]
        fn verify_map() {
            let map: Vec<i32> = vec![];
            assert!(map.is_empty(), "message");
        }
    }
}

amenable_derive::harness! {
    kani, VERIFY_DEQUE_SRC, {
        #[kani::proof]
        fn verify_deque() {
            let dq: Vec<i32> = vec![];
            assert!(dq.is_empty(), "message");
        }
    }
}

amenable_derive::harness! {
    kani, VERIFY_HEAP_SRC, {
        #[kani::proof]
        fn verify_heap() {
            let heap: Vec<i32> = vec![];
            assert!(heap.is_empty(), "message");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write cluster sources")?;

    fs::write(
        src_root.join("alloc_ffi.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_CSTRING_SRC, {
        #[kani::proof]
        fn verify_cstring() {
            let value: i32 = kani::any();
            assert!(value != 0, "different shape");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write lone source")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(workspace.path())
        .with_store_root(store.path())
        .register(&ANTIPATTERNS_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("antipatterns session")?;

    let checklist = fs::read_to_string(store.path().join("findings/antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("read checklist")?;

    assert!(checklist.contains("Possible duplicate clusters"));
    assert!(checklist.contains("X . is_empty ()"));
    assert!(checklist.contains("3 sites"));
    assert!(checklist.contains("verify_map"));
    assert!(checklist.contains("verify_deque"));
    assert!(checklist.contains("verify_heap"));
    let cluster_section = checklist
        .split("Possible duplicate clusters")
        .nth(1)
        .ok_or_else(|| miette::miette!("cluster section present"))?;
    let cluster_block = cluster_section.split("\n\n").next().unwrap_or_default();
    assert!(!cluster_block.contains("verify_cstring"));
    Ok(())
}

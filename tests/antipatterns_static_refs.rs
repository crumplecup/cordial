//! `&'static` fields: which types are flagged, the strategy offered, and who may keep them.

use cordial::{
    AntipatternRuleId, StaticRefStrategy, scan_antipatterns_rust_source_with_static_ref_strategy,
};
use miette::IntoDiagnostic;
use std::path::Path;

#[path = "support/antipatterns_scan.rs"]
mod antipatterns_scan;

#[path = "support/antipatterns_fixture.rs"]
mod antipatterns_fixture;

use antipatterns_fixture::fixture;
use antipatterns_scan::scan_fixture;

fn scan_fixture_with_static_ref_strategy(
    name: &str,
    strategy: StaticRefStrategy,
) -> miette::Result<Vec<cordial::AntipatternSiteRecord>> {
    let src_root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/quality/antipatterns"
    ));
    let file = src_root.join(name);
    scan_antipatterns_rust_source_with_static_ref_strategy(
        &fixture(name)?,
        &file,
        src_root,
        src_root,
        strategy,
    )
    .into_diagnostic()
}

#[test]
fn static_struct_fields_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("static_struct_fields.rs")?;
    let static_refs = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StructStaticRef001)
        .collect::<Vec<_>>();
    assert_eq!(static_refs.len(), 9);
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().contains("BorrowsStatic") && f.context().ends_with("::name"))
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().contains("CapturesLocation")
                && f.snippet().contains("copy `file` and `line` from Location"))
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().contains("Message::Inline") && f.context().ends_with("::_0"))
    );
    assert!(
        !static_refs
            .iter()
            .any(|f| f.context().contains("OwnsData") || f.context().contains("Named::detail"))
    );
    Ok(())
}

#[test]
fn static_struct_field_strategy_defaults_to_string_guidance() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture_with_static_ref_strategy(
        "static_struct_fields.rs",
        StaticRefStrategy::String,
    )?;
    let name = findings
        .iter()
        .find(|f| {
            f.rule_id() == AntipatternRuleId::StructStaticRef001
                && f.context().ends_with("BorrowsStatic::name")
        })
        .ok_or_else(|| miette::miette!("BorrowsStatic::name finding"))?;
    assert!(name.snippet().contains("owned String data"));
    Ok(())
}

#[test]
fn static_struct_field_strategy_can_recommend_cow() -> miette::Result<()> {
    cordial::init_tracing();
    let findings =
        scan_fixture_with_static_ref_strategy("static_struct_fields.rs", StaticRefStrategy::Cow)?;
    let name = findings
        .iter()
        .find(|f| {
            f.rule_id() == AntipatternRuleId::StructStaticRef001
                && f.context().ends_with("BorrowsStatic::name")
        })
        .ok_or_else(|| miette::miette!("BorrowsStatic::name finding"))?;
    assert!(name.snippet().contains("Cow<'static, str>"));
    Ok(())
}

#[test]
fn static_struct_field_strategy_falls_back_to_cow_for_runtime_types() -> miette::Result<()> {
    cordial::init_tracing();
    let findings =
        scan_fixture_with_static_ref_strategy("static_struct_fields.rs", StaticRefStrategy::Const)?;
    let name = findings
        .iter()
        .find(|f| {
            f.rule_id() == AntipatternRuleId::StructStaticRef001
                && f.context().ends_with("BorrowsStatic::name")
        })
        .ok_or_else(|| miette::miette!("BorrowsStatic::name finding"))?;
    assert!(
        name.snippet()
            .contains("const/static only for const-only types")
    );
    assert!(name.snippet().contains("Cow<'static, str>"));
    Ok(())
}

#[test]
fn enum_variant_tuple_and_struct_payloads_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("static_struct_fields.rs")?;
    let variant_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule_id() == AntipatternRuleId::StructStaticRef001
                && (f.context().contains("Message::") || f.context().contains("Payload::"))
        })
        .collect();
    assert_eq!(variant_findings.len(), 5);
    Ok(())
}

#[test]
fn fn_signatures_with_static_refs_are_not_struct_fields() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("static_struct_fields.rs")?;
    assert!(!findings.iter().any(|f| {
        f.rule_id() == AntipatternRuleId::StructStaticRef001
            && (f.context().contains("accepts") || f.context().contains("returns"))
    }));
    Ok(())
}

#[test]
fn crate_local_dyn_trait_static_refs_are_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("local_dyn_trait_static_refs.rs")?;
    let static_refs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StructStaticRef001)
        .collect();
    assert!(
        !static_refs.iter().any(|f| {
            f.context().contains("View")
                || f.context().contains("Registry")
                || f.context().contains("Plugin")
                || f.context().ends_with("Mixed::probes")
        }),
        "local dyn trait tables must be exempt: {static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().ends_with("Mixed::name")),
        "{static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().contains("ForeignDyn")),
        "{static_refs:?}"
    );
    assert_eq!(static_refs.len(), 2);
    Ok(())
}

#[test]
fn const_static_only_types_may_store_static_str() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("const_table_static_refs.rs")?;
    let static_refs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StructStaticRef001)
        .collect();
    assert!(
        !static_refs.iter().any(|f| f.context().contains("ConstRow")),
        "const/static tables must be exempt: {static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().ends_with("RuntimeRow::name")),
        "{static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().ends_with("MixedUse::name")),
        "mixed const+runtime construction must own: {static_refs:?}"
    );
    assert!(
        static_refs.iter().any(|f| {
            f.context().contains("ConstLoc")
                && f.snippet().contains("copy `file` and `line` from Location")
        }),
        "Location on a const type must still be copied out: {static_refs:?}"
    );
    assert_eq!(static_refs.len(), 3);
    Ok(())
}

#[test]
fn inventory_collect_registered_types_may_store_static_str() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("inventory_collect_static_refs.rs")?;
    let static_refs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StructStaticRef001)
        .collect();
    assert!(
        !static_refs
            .iter()
            .any(|f| f.context().ends_with("::Registered::name")),
        "inventory::collect!'d types must be exempt: {static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().ends_with("::NotRegistered::name")),
        "types with no inventory::collect! and real runtime construction must still be flagged: {static_refs:?}"
    );
    assert_eq!(static_refs.len(), 1);
    Ok(())
}

#[test]
fn trait_static_slice_or_ref_return_types_may_store_static_str() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("trait_static_slice_return_static_refs.rs")?;
    let static_refs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::StructStaticRef001)
        .collect();
    assert!(
        !static_refs
            .iter()
            .any(|f| f.context().ends_with("::Transition::name")),
        "a type promised by a trait's own -> &'static [Type]/&'static Type \
         return signature must be exempt: {static_refs:?}"
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context().ends_with("::NotPromised::name")),
        "{static_refs:?}"
    );
    assert_eq!(static_refs.len(), 1);
    Ok(())
}

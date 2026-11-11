//! The crate walk and its report: pruned galleries, duplicate clusters, and dotted `ensures` calls.

#![cfg(feature = "antipatterns")]

use cordial::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, AntipatternWorkspaceFacts, CrateTarget,
    ErrorFieldStrategy, IrView, RunAll, Session, SessionBuilder, StaticRefStrategy,
    scan_verus_contract_bounds_source,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/contract_bounds_fixtures.rs"]
mod contract_bounds_fixtures;

use contract_bounds_fixtures::fixtures_root;

#[path = "support/source_projection.rs"]
mod source_projection;

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

    fs::write(
        crate_root.path().join("Cargo.toml"),
        "[package]\nname = 'amenable_kani'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let findings = source_projection::baseline(
        crate_root.path(),
        &CrateTarget::new("amenable_kani", crate_root.path()),
    )?
    .source_antipatterns()
    .into_diagnostic()?
    .assess(
        "amenable_kani",
        &AntipatternWorkspaceFacts::new(Vec::new(), std::collections::HashMap::new()),
        StaticRefStrategy::default(),
        ErrorFieldStrategy::default(),
    )
    .into_diagnostic()?
    .into_iter()
    .filter(|row| row.rule_id() == AntipatternRuleId::UnnamedContractBound001)
    .collect::<Vec<_>>();

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context().contains("verify_production"));
    assert!(!findings[0].file().starts_with("src/gallery"));
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
        .register(&*ANTIPATTERNS_ETIQUETTE)
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

#[test]
fn verus_dotted_ensures_method_call_is_not_mistaken_for_the_clause_keyword() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub assume_specification<H: core::default::Default + Hasher> [<BuildHasherDefault<H> as BuildHasher>::build_hasher] (builder: &BuildHasherDefault<H>) -> (result: H)
    ensures
        H::default.ensures((), result),
;

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_dotted_ensures_method.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus dotted-ensures method call")?;

    // Two real properties, both confirmed by the same zero-findings
    // outcome. First (the original, narrower concern this test was
    // written for): the whole `H::default.ensures((), result)` clause
    // has to survive `walk_verus_tokens` as one intact clause, not split
    // in two at the method call's own literal `ensures` identifier --
    // either half on its own (`H :: default .` / `(() , result)`) fails
    // to parse as a real expression, so a split would surface as two
    // spurious flagged fragments instead of zero findings. Second (added
    // once `is_builtin_contract_inspection` recognized this shape):
    // `H::default.ensures(..)` is Verus's own builtin function-item
    // contract inspection, not a call to a registered predicate, and
    // there is no local `fn` it could ever be pointed at -- so the
    // intact clause is correctly exempt, not flagged.
    assert_eq!(findings.len(), 0, "findings: {findings:?}");
    Ok(())
}

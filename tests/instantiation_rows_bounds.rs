//! Checking each parameter against its own bounds, and rows that are not generic.
#![cfg(feature = "amenable_std")]

use std::collections::{HashMap, HashSet};

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{
    AmenableRegistryEvidence, AmenableStdEntry, AmenableStdReport, AmenableStdStatus,
    ExpectedInstantiations, InstantiationContext, RegistryDump, ResolveCaps, RustdocTypeResolver,
    VerifierSkipMap, expand_report,
};

#[path = "support/instantiation_world.rs"]
mod instantiation_world;

#[path = "support/instantiation_reports.rs"]
mod instantiation_reports;

#[path = "support/instantiation_expand.rs"]
mod instantiation_expand;

use instantiation_expand::{all_four, expand, registry};
use instantiation_reports::{WITNESSED, expected, find, four_zones, note, parent_entry, report};
use instantiation_world::{Krate, resolver};

fn plain_entry(path: &str, status: AmenableStdStatus) -> miette::Result<AmenableStdEntry> {
    AmenableStdEntry::builder()
        .type_path(path.to_string())
        .type_kind("struct".to_string())
        .is_generic(false)
        .evidence_link(true)
        .evidence_name(None)
        .kani_witness(true)
        .creusot_witness(true)
        .verus_witness(true)
        .proof_test(false)
        .status(status)
        .skip_reason(None)
        .kani_excepted(false)
        .creusot_excepted(false)
        .verus_excepted(false)
        .build()
        .into_diagnostic()
}

impl Krate {
    /// A struct defined in a private module and re-exported from the root,
    /// as chrono does: rustdoc records the defining path but strips the
    /// private module from the tree.
    fn reexported_struct(&mut self, id: u32, name: &str, private_module: &str, impls: &[u32]) {
        self.item(
            id,
            Some(name),
            json!({"struct": {
                "kind": {"plain": {"fields": [], "has_stripped_fields": false}},
                "generics": Self::generics(&[]), "impls": impls,
            }}),
        );
        let crate_name = self.name.clone();
        self.path(
            id,
            0,
            &[crate_name.as_str(), private_module, name],
            "struct",
        );
        self.item(
            id + 1000,
            None,
            json!({"use": {"source": name, "name": name, "id": id, "is_glob": false}}),
        );
        self.root_items.push(id + 1000);
    }
}

/// `Map<K, V>` with different bounds per parameter: `K: Hash + Eq` inline and
/// `V: Clone` in a `where` clause, as a library writes them.
fn map_resolver() -> miette::Result<RustdocTypeResolver> {
    let mut k = Krate::new("lib");
    k.trait_item(5, "Hash");
    k.trait_item(6, "Eq");
    k.trait_item(7, "Clone");
    k.strukt(2, "Key1", &[], &[20, 21]);
    k.strukt(3, "Key2", &[], &[22]);
    k.strukt(4, "Val1", &[], &[23]);
    k.strukt(8, "Val2", &[], &[]);
    k.strukt(10, "Map", &["K", "V"], &[]);
    k.declare_bound(10, "K", 5, "Hash", false);
    k.declare_bound(10, "K", 6, "Eq", false);
    k.declare_bound(10, "V", 7, "Clone", true);
    k.impl_of(20, 5, "Hash", 2, "Key1");
    k.impl_of(21, 6, "Eq", 2, "Key1");
    k.impl_of(22, 5, "Hash", 3, "Key2");
    k.impl_of(23, 7, "Clone", 4, "Val1");
    Ok(RustdocTypeResolver::new(
        vec![k.build()?],
        ResolveCaps::default(),
    ))
}

/// Expand a one-parent report for the `lib::Map` generic.
fn expand_map(
    resolver: &RustdocTypeResolver,
    claims: &[&str],
) -> miette::Result<AmenableStdReport> {
    let links: Vec<Value> = claims
        .iter()
        .map(|ty| json!({"name": format!("amenable_ext::ExtStandard<{ty}>"), "basis": "", "index": 0}))
        .collect();
    let dump: RegistryDump = serde_json::from_value(json!({
        "evidence_links": links, "proof_records": [], "kani_proofs": [],
    }))
    .into_diagnostic()?;
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = ExpectedInstantiations::default();
    let ctx = InstantiationContext::new(resolver, &facts, &skip, &wanted);
    expand_report(&report(vec![parent_entry("lib::Map")?])?, &ctx).into_diagnostic()
}

#[test]
fn each_parameters_bounds_are_checked_against_its_own_argument() -> miette::Result<()> {
    cordial::init_tracing();
    let resolver = map_resolver()?;
    let out = expand_map(
        &resolver,
        &[
            "lib::Map<lib::Key1, lib::Val1>",
            "lib::Map<lib::Key2, lib::Val1>",
            "lib::Map<lib::Key1, lib::Val2>",
        ],
    )?;
    let text = note(find(&out, "lib::Map")?);
    assert!(
        text.contains("declared bounds `K: Hash + Eq`, `V: Clone`"),
        "{text}"
    );
    assert!(text.contains("satisfied by Key1, Val1"), "{text}");
    // Key2 has Hash but not Eq: the failure names the parameter and bound.
    assert!(text.contains("Key2, Val1 (K: Eq)"), "{text}");
    // Val2 has no Clone: a different parameter, a different bound.
    assert!(text.contains("Key1, Val2 (V: Clone)"), "{text}");
    Ok(())
}

#[test]
fn a_type_that_declares_no_bounds_says_so_instead_of_guessing() -> miette::Result<()> {
    cordial::init_tracing();
    // A generic with no declared bounds is the `HashMap` case: its bounds sit
    // on impl blocks, not on the type.
    let mut k = Krate::new("lib");
    k.strukt(2, "Key1", &[], &[]);
    k.strukt(10, "Map", &["K", "V"], &[]);
    let resolver = RustdocTypeResolver::new(vec![k.build()?], ResolveCaps::default());
    let out = expand_map(&resolver, &["lib::Map<lib::Key1, lib::Key1>"])?;
    let text = note(find(&out, "lib::Map")?);
    assert!(
        text.contains("declares no bounds on its parameters"),
        "{text}"
    );
    Ok(())
}

#[test]
fn proof_chain_tests_are_matched_per_instantiation_even_when_written_bare() -> miette::Result<()> {
    cordial::init_tracing();
    let subjects = HashSet::from(["ExtStandard<DateTime<Utc>>".to_string()]);
    let out = expand(&all_four(), &four_zones(), &HashMap::new(), &subjects)?;
    assert!(find(&out, "chrono::DateTime<chrono::Utc>")?.proof_test());
    assert!(!find(&out, "chrono::DateTime<chrono::FixedOffset>")?.proof_test());
    Ok(())
}

#[test]
fn rows_that_are_not_generic_pass_through_and_the_report_is_recounted() -> miette::Result<()> {
    cordial::init_tracing();
    let resolver = resolver()?;
    let dump = registry(&all_four())?;
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), &resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = four_zones();
    let ctx = InstantiationContext::new(&resolver, &facts, &skip, &wanted);
    let input = report(vec![
        plain_entry("chrono::Utc", AmenableStdStatus::Complete)?,
        parent_entry("chrono::DateTime")?,
        plain_entry("chrono::Bare", AmenableStdStatus::Missing)?,
    ])?;
    let out = expand_report(&input, &ctx).into_diagnostic()?;
    assert_eq!(out.entries().len(), 7);
    assert_eq!(
        find(&out, "chrono::Bare")?.status(),
        AmenableStdStatus::Missing
    );
    assert_eq!(
        out.complete_count(),
        6,
        "Utc, the parent, and four children"
    );
    assert_eq!(out.missing_count(), 1);
    Ok(())
}

#[test]
fn the_bound_check_finds_a_type_defined_in_a_private_module() -> miette::Result<()> {
    cordial::init_tracing();
    let mut k = Krate::new("chrono");
    k.trait_item(5, "TimeZone");
    k.reexported_struct(2, "Utc", "utc", &[20]);
    k.strukt(10, "DateTime", &["Tz"], &[]);
    k.declare_bound(10, "Tz", 5, "TimeZone", false);
    k.impl_of(20, 5, "TimeZone", 2, "Utc");
    let resolver = RustdocTypeResolver::new(vec![k.build()?], ResolveCaps::default());
    let dump = registry(&[("chrono::DateTime<chrono::Utc>", &WITNESSED)])?;
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), &resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = expected(&["chrono::Utc"]);
    let ctx = InstantiationContext::new(&resolver, &facts, &skip, &wanted);
    let out =
        expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()?;
    let text = note(find(&out, "chrono::DateTime")?);
    // The canonical head is the defining path, `chrono::utc::Utc`.
    assert!(text.contains("satisfied by Utc"), "{text}");
    Ok(())
}

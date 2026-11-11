//! Deriving instantiations: multi-parameter products, the combination cap, undeclared bounds, and exceptions.
#![cfg(feature = "amenable_std")]

use std::collections::{HashMap, HashSet};

use miette::IntoDiagnostic;
use serde_json::json;

use cordial::testing::{
    AmenableRegistryEvidence, AmenableStdReport, AmenableStdStatus, ExpectedInstantiations,
    InstantiationContext, RegistryDump, ResolveCaps, RustdocTypeResolver, VerifierSkipEntry,
    VerifierSkipMap, expand_report,
};

#[path = "support/instantiation_world.rs"]
mod instantiation_world;

#[path = "support/instantiation_reports.rs"]
mod instantiation_reports;

#[path = "support/instantiation_expand.rs"]
mod instantiation_expand;

use instantiation_expand::{all_four, expand};
use instantiation_reports::{WITNESSED, expected, find, four_zones, note, parent_entry, report};
use instantiation_world::Krate;

/// `Map<K: Hash + Eq, V: Clone>` with `keys` hashable keys and `vals` clonable
/// values, plus one key that is only `Hash`.
fn map_resolver_sized(keys: u32, vals: u32) -> miette::Result<RustdocTypeResolver> {
    let mut k = Krate::new("lib");
    k.trait_item(5, "Hash");
    k.trait_item(6, "Eq");
    k.trait_item(7, "Clone");
    k.strukt(10, "Map", &["K", "V"], &[]);
    k.declare_bound(10, "K", 5, "Hash", false);
    k.declare_bound(10, "K", 6, "Eq", false);
    k.declare_bound(10, "V", 7, "Clone", true);
    let mut next_impl = 100;
    for key in 0..keys {
        let id = 20 + key;
        k.strukt(id, &format!("Key{key}"), &[], &[next_impl, next_impl + 1]);
        k.impl_of(next_impl, 5, "Hash", id, "Key");
        k.impl_of(next_impl + 1, 6, "Eq", id, "Key");
        next_impl += 2;
    }
    k.strukt(40, "OnlyHash", &[], &[next_impl]);
    k.impl_of(next_impl, 5, "Hash", 40, "OnlyHash");
    next_impl += 1;
    for val in 0..vals {
        let id = 50 + val;
        k.strukt(id, &format!("Val{val}"), &[], &[next_impl]);
        k.impl_of(next_impl, 7, "Clone", id, "Val");
        next_impl += 1;
    }
    Ok(RustdocTypeResolver::new(
        vec![k.build()?],
        ResolveCaps::default(),
    ))
}

fn expand_map_derived(
    resolver: &RustdocTypeResolver,
    cap: usize,
) -> miette::Result<AmenableStdReport> {
    let dump: RegistryDump = serde_json::from_value(json!({
        "evidence_links": [], "proof_records": [], "kani_proofs": [],
    }))
    .into_diagnostic()?;
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = ExpectedInstantiations::default().with_derive_cap(cap);
    let ctx = InstantiationContext::new(resolver, &facts, &skip, &wanted);
    expand_report(&report(vec![parent_entry("lib::Map")?])?, &ctx).into_diagnostic()
}

#[test]
fn multi_parameter_candidates_are_the_product_of_each_parameters_implementors() -> miette::Result<()>
{
    cordial::init_tracing();
    // Two keys that are Hash + Eq (a third is only Hash, so it is not a
    // candidate) times two clonable values: four instantiations.
    let resolver = map_resolver_sized(2, 2)?;
    let out = expand_map_derived(&resolver, 16)?;
    assert_eq!(out.entries().len(), 5, "{:?}", out.entries());
    assert!(find(&out, "lib::Map<lib::Key0, lib::Val1>").is_ok());
    assert!(
        out.entries()
            .iter()
            .all(|entry| !entry.type_path().contains("OnlyHash"))
    );
    Ok(())
}

#[test]
fn too_many_combinations_ask_for_a_list_instead_of_generating_rows() -> miette::Result<()> {
    cordial::init_tracing();
    let resolver = map_resolver_sized(2, 2)?;
    let out = expand_map_derived(&resolver, 3)?;
    assert_eq!(out.entries().len(), 1, "no rows are generated over the cap");
    let text = note(find(&out, "lib::Map")?);
    assert!(text.contains("exceed the cap of 3"), "{text}");
    assert!(text.contains("list the instantiations to track"), "{text}");
    Ok(())
}

#[test]
fn a_parameter_with_no_declared_bound_cannot_be_derived() -> miette::Result<()> {
    cordial::init_tracing();
    let mut k = Krate::new("lib");
    k.trait_item(5, "Hash");
    k.strukt(10, "Map", &["K", "V"], &[]);
    k.declare_bound(10, "K", 5, "Hash", false);
    // `K` has a candidate; `V` declares nothing, so there is nothing to pick.
    k.strukt(2, "Key0", &[], &[20]);
    k.impl_of(20, 5, "Hash", 2, "Key0");
    let resolver = RustdocTypeResolver::new(vec![k.build()?], ResolveCaps::default());
    let out = expand_map_derived(&resolver, 16)?;
    let text = note(find(&out, "lib::Map")?);
    assert!(text.contains("parameter `V` declares no bounds"), "{text}");
    Ok(())
}

#[test]
fn an_expected_type_that_cannot_be_identified_is_a_missing_row_with_the_reason()
-> miette::Result<()> {
    cordial::init_tracing();
    let out = expand(
        &all_four(),
        &expected(&["chrono::Utc", "chrono::Nowhere"]),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    let unknown = find(&out, "chrono::DateTime<chrono::Nowhere>")?;
    assert_eq!(unknown.status(), AmenableStdStatus::Missing);
    assert!(
        note(unknown).contains("could not be identified"),
        "{}",
        note(unknown)
    );
    Ok(())
}

#[test]
fn an_excepted_instantiation_does_not_block_the_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.remove(2); // Local has no coverage, but is excepted
    let skip: VerifierSkipMap = HashMap::from([(
        "chrono::DateTime<chrono::Local>".to_string(),
        VerifierSkipEntry::new("Local has no stable offset".to_string(), None),
    )]);
    let out = expand(&claims, &four_zones(), &skip, &HashSet::new())?;
    assert_eq!(
        find(&out, "chrono::DateTime<chrono::Local>")?.status(),
        AmenableStdStatus::Skipped
    );
    let parent = find(&out, "chrono::DateTime")?;
    assert_eq!(parent.status(), AmenableStdStatus::Complete);
    assert!(
        note(parent).starts_with("3 of 3 instantiations Complete (1 excepted)"),
        "{}",
        note(parent)
    );
    Ok(())
}

#[test]
fn the_parent_note_lists_who_meets_the_types_declared_bounds() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.push(("chrono::DateTime<chrono::Bare>", &WITNESSED));
    let out = expand(&claims, &four_zones(), &HashMap::new(), &HashSet::new())?;
    let text = note(find(&out, "chrono::DateTime")?);
    // Read from chrono's own definition, `struct DateTime<Tz: TimeZone>`:
    // nothing about the bound comes from the registry.
    assert!(text.contains("declared bounds `Tz: TimeZone`"), "{text}");
    // Utc, FixedOffset and Local implement it in chrono; Tz in chrono_tz,
    // through an external trait id.
    assert!(
        text.contains("satisfied by Utc, FixedOffset, Local, Tz"),
        "{text}"
    );
    assert!(
        text.contains("no direct impl found for Bare (Tz: TimeZone)"),
        "{text}"
    );
    Ok(())
}

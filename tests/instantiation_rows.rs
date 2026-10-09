#![cfg(feature = "amenable_std")]

//! Per-instantiation coverage rows: a generic row becomes an aggregate
//! parent plus one child per instantiation, each with its own status.

use std::collections::{BTreeMap, HashMap, HashSet};

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{
    AmenableStdEntry, AmenableStdReport, AmenableStdStatus, CrateIndex, ExpectedInstantiations,
    InstantiationContext, RegistryDump, RegistryFacts, ResolveCaps, RustdocTypeResolver,
    VerifierSkipEntry, VerifierSkipMap, expand_report,
};

// ---- a tiny rustdoc crate builder ----------------------------------------

struct Krate {
    name: String,
    index: serde_json::Map<String, Value>,
    paths: serde_json::Map<String, Value>,
    root_items: Vec<u32>,
}

impl Krate {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            index: serde_json::Map::new(),
            paths: serde_json::Map::new(),
            root_items: Vec::new(),
        }
    }

    fn item(&mut self, id: u32, name: Option<&str>, inner: Value) {
        self.index.insert(
            id.to_string(),
            json!({
                "id": id, "crate_id": 0, "name": name, "span": null,
                "visibility": "public", "docs": null, "links": {}, "attrs": [],
                "deprecation": null, "inner": inner,
            }),
        );
    }

    fn path(&mut self, id: u32, crate_id: u32, path: &[&str], kind: &str) {
        self.paths.insert(
            id.to_string(),
            json!({"crate_id": crate_id, "path": path, "kind": kind}),
        );
    }

    fn generics(params: &[&str]) -> Value {
        let params: Vec<Value> = params
            .iter()
            .map(|p| {
                json!({"name": p, "kind": {"type": {"bounds": [], "default": null, "is_synthetic": false}}})
            })
            .collect();
        json!({"params": params, "where_predicates": []})
    }

    /// A struct in the crate root, with the given trait-impl item ids.
    fn strukt(&mut self, id: u32, name: &str, params: &[&str], impls: &[u32]) {
        self.item(
            id,
            Some(name),
            json!({"struct": {
                "kind": {"plain": {"fields": [], "has_stripped_fields": false}},
                "generics": Self::generics(params), "impls": impls,
            }}),
        );
        let crate_name = self.name.clone();
        self.path(id, 0, &[crate_name.as_str(), name], "struct");
        self.root_items.push(id);
    }

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

    /// Declare `param: trait_name` on the struct `id`, inline
    /// (`struct S<P: Trait>`) or, if `in_where`, in a `where` clause.
    fn declare_bound(
        &mut self,
        id: u32,
        param: &str,
        trait_id: u32,
        trait_name: &str,
        in_where: bool,
    ) {
        let bound = json!({"trait_bound": {
            "trait": {"path": trait_name, "id": trait_id, "args": null},
            "generic_params": [], "modifier": "none",
        }});
        let Some(generics) = self
            .index
            .get_mut(&id.to_string())
            .and_then(|item| item.pointer_mut("/inner/struct/generics"))
        else {
            return;
        };
        if in_where {
            if let Some(Value::Array(predicates)) = generics.get_mut("where_predicates") {
                predicates.push(json!({"bound_predicate": {
                    "type": {"generic": param}, "bounds": [bound], "generic_params": [],
                }}));
            }
        } else if let Some(Value::Array(params)) = generics.get_mut("params") {
            for found in params.iter_mut().filter(|p| p["name"] == param) {
                if let Some(Value::Array(bounds)) = found.pointer_mut("/kind/type/bounds") {
                    bounds.push(bound.clone());
                }
            }
        }
    }

    fn enumeration(&mut self, id: u32, name: &str, impls: &[u32]) {
        self.item(
            id,
            Some(name),
            json!({"enum": {
                "generics": Self::generics(&[]), "has_stripped_variants": false,
                "variants": [], "impls": impls,
            }}),
        );
        let crate_name = self.name.clone();
        self.path(id, 0, &[crate_name.as_str(), name], "enum");
        self.root_items.push(id);
    }

    fn trait_item(&mut self, id: u32, name: &str) {
        self.item(
            id,
            Some(name),
            json!({"trait": {
                "is_auto": false, "is_unsafe": false, "is_dyn_compatible": true,
                "items": [], "generics": Self::generics(&[]), "bounds": [],
                "implementations": [],
            }}),
        );
        let crate_name = self.name.clone();
        self.path(id, 0, &[crate_name.as_str(), name], "trait");
        self.root_items.push(id);
    }

    /// `impl <trait> for <type>`; `trait_id` may be an external id.
    fn impl_of(&mut self, id: u32, trait_id: u32, trait_name: &str, type_id: u32, type_name: &str) {
        self.item(
            id,
            None,
            json!({"impl": {
                "is_unsafe": false, "generics": Self::generics(&[]),
                "provided_trait_methods": [],
                "trait": {"path": trait_name, "id": trait_id, "args": null},
                "for": {"resolved_path": {"path": type_name, "id": type_id, "args": null}},
                "items": [], "is_negative": false, "is_synthetic": false,
                "blanket_impl": null,
            }}),
        );
    }

    fn build(mut self) -> miette::Result<CrateIndex> {
        let root_items = self.root_items.clone();
        let name = self.name.clone();
        self.item(
            0,
            Some(&name),
            json!({"module": {"is_crate": true, "items": root_items, "is_stripped": false}}),
        );
        self.path(0, 0, &[name.as_str()], "module");
        let document = json!({
            "root": 0, "crate_version": "0.0.0", "includes_private": false,
            "index": self.index, "paths": self.paths, "external_crates": {},
            "target": {"triple": "x86_64-unknown-linux-gnu", "target_features": []},
            "format_version": 57,
        });
        let krate = serde_json::from_value(document).into_diagnostic()?;
        Ok(CrateIndex::new(&name, krate))
    }
}

/// chrono-shaped: `DateTime<Tz>`, the zones `Utc` / `FixedOffset` / `Local`
/// (each with `impl TimeZone`), and `Bare`, which has no such impl.
fn chrono() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono");
    k.trait_item(5, "TimeZone");
    k.strukt(2, "Utc", &[], &[20]);
    k.strukt(3, "FixedOffset", &[], &[21]);
    k.strukt(4, "Local", &[], &[22]);
    k.strukt(6, "Bare", &[], &[]);
    k.strukt(10, "DateTime", &["Tz"], &[]);
    k.declare_bound(10, "Tz", 5, "TimeZone", false);
    k.impl_of(20, 5, "TimeZone", 2, "Utc");
    k.impl_of(21, 5, "TimeZone", 3, "FixedOffset");
    k.impl_of(22, 5, "TimeZone", 4, "Local");
    k.build()
}

/// chrono_tz-shaped: `Tz` implements chrono's `TimeZone`, named by an
/// external id (the trait lives in chrono).
fn chrono_tz() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono_tz");
    k.enumeration(2, "Tz", &[20]);
    k.path(7, 1, &["chrono", "TimeZone"], "trait");
    k.impl_of(20, 7, "TimeZone", 2, "Tz");
    k.build()
}

// ---- fixtures ---------------------------------------------------------------

const WITNESSED: [&str; 3] = ["kani", "creusot", "verus"];

fn resolver() -> miette::Result<RustdocTypeResolver> {
    Ok(RustdocTypeResolver::new(
        vec![chrono()?, chrono_tz()?],
        ResolveCaps::default(),
    ))
}

/// A registry dump with these concrete claims (each with the verifiers that
/// have a witness for it) plus the generic `DateTime<Tz>` claim.
fn registry(claims: &[(&str, &[&str])]) -> miette::Result<RegistryDump> {
    let links: Vec<Value> = claims
        .iter()
        .map(|(ty, _)| {
            json!({"name": format!("amenable_ext::ExtStandard<{ty}>"), "basis": "", "index": 0})
        })
        .collect();
    let proofs: Vec<Value> = claims
        .iter()
        .flat_map(|(ty, verifiers)| {
            verifiers.iter().map(move |verifier| {
                json!({"evidence": format!("amenable_ext::ExtStandard<{ty}>"), "verifier": verifier})
            })
        })
        .collect();
    serde_json::from_value(json!({
        "evidence_links": links, "proof_records": proofs, "kani_proofs": [],
    }))
    .into_diagnostic()
}

fn parent_entry(path: &str) -> miette::Result<AmenableStdEntry> {
    AmenableStdEntry::builder()
        .type_path(path.to_string())
        .type_kind("struct".to_string())
        .is_generic(true)
        .evidence_link(true)
        .evidence_name(None)
        .kani_witness(true)
        .creusot_witness(true)
        .verus_witness(true)
        .proof_test(false)
        .status(AmenableStdStatus::Complete)
        .skip_reason(None)
        .kani_excepted(false)
        .creusot_excepted(false)
        .verus_excepted(false)
        .build()
        .into_diagnostic()
}

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

fn report(entries: Vec<AmenableStdEntry>) -> miette::Result<AmenableStdReport> {
    AmenableStdReport::builder()
        .source_crate("chrono".to_string())
        .impl_crate("amenable_ext".to_string())
        .include_nightly(false)
        .entries(entries)
        .complete_count(0)
        .partial_count(0)
        .missing_count(0)
        .skipped_count(0)
        .build()
        .into_diagnostic()
}

fn expected(tuples: &[&str]) -> ExpectedInstantiations {
    ExpectedInstantiations::new(BTreeMap::from([(
        "chrono::DateTime".to_string(),
        tuples.iter().map(|t| vec![(*t).to_string()]).collect(),
    )]))
}

fn four_zones() -> ExpectedInstantiations {
    expected(&[
        "chrono::Utc",
        "chrono::FixedOffset",
        "chrono::Local",
        "chrono_tz::Tz",
    ])
}

/// Expand a one-parent report and return its rows.
fn expand(
    claims: &[(&str, &[&str])],
    expected: &ExpectedInstantiations,
    skip_map: &VerifierSkipMap,
    proof_subjects: &HashSet<String>,
) -> miette::Result<AmenableStdReport> {
    let resolver = resolver()?;
    let dump = registry(claims)?;
    let facts = RegistryFacts::resolve(&dump, proof_subjects, &resolver);
    let ctx = InstantiationContext::new(&resolver, &facts, skip_map, expected);
    expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()
}

fn all_four() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("chrono::DateTime<chrono::Utc>", &WITNESSED),
        ("chrono::DateTime<chrono::FixedOffset>", &WITNESSED),
        ("chrono::DateTime<chrono::Local>", &WITNESSED),
        ("chrono::DateTime<chrono_tz::Tz>", &WITNESSED),
    ]
}

fn find<'a>(report: &'a AmenableStdReport, path: &str) -> miette::Result<&'a AmenableStdEntry> {
    report
        .entries()
        .iter()
        .find(|entry| entry.type_path() == path)
        .ok_or_else(|| miette::miette!("no row `{path}`"))
}

fn note(entry: &AmenableStdEntry) -> String {
    entry.note().clone().unwrap_or_default()
}

// ---- tests ------------------------------------------------------------------

#[test]
fn four_covered_zones_make_a_complete_parent_with_four_complete_children() -> miette::Result<()> {
    cordial::init_tracing();
    let out = expand(&all_four(), &four_zones(), &HashMap::new(), &HashSet::new())?;
    assert_eq!(out.entries().len(), 5, "{:?}", out.entries());

    let parent = find(&out, "chrono::DateTime")?;
    assert_eq!(parent.status(), AmenableStdStatus::Complete);
    assert_eq!(parent.parent(), &None);
    assert!(
        note(parent).starts_with("4 of 4 instantiations Complete"),
        "{}",
        note(parent)
    );

    for label in [
        "chrono::DateTime<chrono::Utc>",
        "chrono::DateTime<chrono::FixedOffset>",
        "chrono::DateTime<chrono_tz::Tz>",
    ] {
        let child = find(&out, label)?;
        assert_eq!(child.status(), AmenableStdStatus::Complete, "{label}");
        assert_eq!(child.parent().as_deref(), Some("chrono::DateTime"));
        assert!(child.evidence_link() && child.kani_witness());
    }
    assert_eq!(out.complete_count(), 5);
    Ok(())
}

#[test]
fn children_follow_the_configured_order_after_the_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let out = expand(&all_four(), &four_zones(), &HashMap::new(), &HashSet::new())?;
    let paths: Vec<&str> = out
        .entries()
        .iter()
        .map(|e| e.type_path().as_str())
        .collect();
    assert_eq!(
        paths,
        vec![
            "chrono::DateTime",
            "chrono::DateTime<chrono::Utc>",
            "chrono::DateTime<chrono::FixedOffset>",
            "chrono::DateTime<chrono::Local>",
            "chrono::DateTime<chrono_tz::Tz>",
        ]
    );
    Ok(())
}

#[test]
fn an_uncovered_zone_is_a_missing_child_and_the_parent_is_partial() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.remove(2); // no Local
    let out = expand(&claims, &four_zones(), &HashMap::new(), &HashSet::new())?;

    let local = find(&out, "chrono::DateTime<chrono::Local>")?;
    assert_eq!(local.status(), AmenableStdStatus::Missing);
    assert!(!local.evidence_link());

    let parent = find(&out, "chrono::DateTime")?;
    assert_eq!(parent.status(), AmenableStdStatus::Partial);
    assert!(note(parent).starts_with("3 of 4 instantiations Complete"));
    assert!(
        !parent.evidence_link(),
        "an aggregate column is set only if every child has it"
    );
    assert_eq!(
        (
            out.complete_count(),
            out.missing_count(),
            out.partial_count()
        ),
        (3, 1, 1),
        "three complete children; one missing child; the partial parent"
    );
    Ok(())
}

#[test]
fn witnesses_are_attributed_to_the_instantiation_they_prove() -> miette::Result<()> {
    cordial::init_tracing();
    let claims: Vec<(&str, &[&str])> = vec![
        ("chrono::DateTime<chrono::Utc>", &["kani"]),
        ("chrono::DateTime<chrono::FixedOffset>", &WITNESSED),
    ];
    let out = expand(
        &claims,
        &expected(&["chrono::Utc", "chrono::FixedOffset"]),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    let utc = find(&out, "chrono::DateTime<chrono::Utc>")?;
    assert_eq!(
        utc.status(),
        AmenableStdStatus::Partial,
        "evidence but only kani"
    );
    assert!(utc.kani_witness() && !utc.creusot_witness() && !utc.verus_witness());
    let fixed = find(&out, "chrono::DateTime<chrono::FixedOffset>")?;
    assert_eq!(fixed.status(), AmenableStdStatus::Complete);
    assert_eq!(
        find(&out, "chrono::DateTime")?.status(),
        AmenableStdStatus::Partial
    );
    Ok(())
}

#[test]
fn a_registered_instantiation_outside_the_expected_list_is_still_shown() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.push(("chrono::DateTime<chrono::Bare>", &WITNESSED));
    let out = expand(&claims, &four_zones(), &HashMap::new(), &HashSet::new())?;
    let extra = find(&out, "chrono::DateTime<chrono::Bare>")?;
    assert!(
        note(extra).contains("not in the expected list"),
        "{}",
        note(extra)
    );
    assert_eq!(extra.status(), AmenableStdStatus::Complete);
    Ok(())
}

#[test]
fn without_an_expected_list_the_registered_instantiations_still_become_rows() -> miette::Result<()>
{
    cordial::init_tracing();
    let claims: Vec<(&str, &[&str])> = vec![
        ("chrono::DateTime<chrono::Utc>", &WITNESSED),
        ("chrono::DateTime<chrono::Local>", &WITNESSED),
    ];
    let out = expand(
        &claims,
        &ExpectedInstantiations::default(),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    assert_eq!(out.entries().len(), 3, "{:?}", out.entries());
    assert!(out.entries().iter().skip(1).all(|e| e.parent().is_some()));
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
    let facts = RegistryFacts::resolve(&dump, &HashSet::new(), resolver);
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
    let facts = RegistryFacts::resolve(&dump, &HashSet::new(), &resolver);
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
    let facts = RegistryFacts::resolve(&dump, &HashSet::new(), &resolver);
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

// ---- real data ---------------------------------------------------------------

/// The cached rustdoc JSON the amenable-ext targets are built from. Absent on
/// machines that never ran a coverage build; the test then has nothing to
/// check and passes.
fn cached(crate_name: &str) -> Option<CrateIndex> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home)
        .join(".cordial/amenable/cache/rustdoc-target/doc")
        .join(format!("{crate_name}.json"));
    path.is_file()
        .then(|| CrateIndex::load(crate_name, &path).ok())?
}

#[test]
fn real_chrono_zones_satisfy_the_timezone_bound_across_crates() -> miette::Result<()> {
    cordial::init_tracing();
    let (Some(chrono), Some(chrono_tz)) = (cached("chrono"), cached("chrono_tz")) else {
        tracing::info!("skipped: no cached chrono / chrono_tz rustdoc JSON");
        return Ok(());
    };
    let resolver = RustdocTypeResolver::new(vec![chrono, chrono_tz], ResolveCaps::default());
    let zones = [
        "chrono::DateTime<chrono::Utc>",
        "chrono::DateTime<chrono::FixedOffset>",
        "chrono::DateTime<chrono::Local>",
        "chrono::DateTime<chrono_tz::Tz>",
    ];
    let links: Vec<Value> = zones
        .iter()
        .map(|ty| json!({"name": format!("amenable_ext::ExtStandard<{ty}>"), "basis": "", "index": 0}))
        .collect();
    let dump: RegistryDump = serde_json::from_value(json!({
        "evidence_links": links, "proof_records": [], "kani_proofs": [],
    }))
    .into_diagnostic()?;
    let facts = RegistryFacts::resolve(&dump, &HashSet::new(), &resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = four_zones();
    let ctx = InstantiationContext::new(&resolver, &facts, &skip, &wanted);
    let out =
        expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()?;

    assert_eq!(out.entries().len(), 5, "{:?}", out.entries());
    let text = note(find(&out, "chrono::DateTime")?);
    assert!(
        text.contains("declared bounds `Tz: TimeZone`: satisfied by Utc, FixedOffset, Local, Tz"),
        "every zone should satisfy chrono's own declared bound: {text}"
    );
    assert!(!text.contains("no direct impl"), "{text}");
    assert!(!text.contains("unknown"), "{text}");
    Ok(())
}

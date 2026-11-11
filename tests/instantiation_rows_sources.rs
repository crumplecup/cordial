//! Any coverage source can drive the expansion, and real chrono data resolves across crates.
#![cfg(feature = "amenable_std")]

use std::collections::{HashMap, HashSet};

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{
    AmenableRegistryEvidence, AmenableStdStatus, CrateIndex, ExpectedInstantiations,
    InstantiationContext, InstantiationEvidence, RegistryDump, ResolveCaps, RustdocTypeResolver,
    TypeKey, TypeResolver, VerifierSkipMap, expand_report,
};

#[path = "support/instantiation_world.rs"]
mod instantiation_world;

#[path = "support/instantiation_reports.rs"]
mod instantiation_reports;

use instantiation_reports::{WITNESSED, find, four_zones, note, parent_entry, report};
use instantiation_world::resolver;

// ---- a coverage source that is not amenable --------------------------------

/// Coverage as some other tool defines it: a fixed set of instantiations it
/// has verified, with nothing in common with amenable's registry. It names no
/// wrapper type, no dump and no proof record.
struct FixedCoverage {
    covered: Vec<TypeKey>,
    /// The instantiations that also have a test exercising them.
    tested: Vec<TypeKey>,
}

impl InstantiationEvidence for FixedCoverage {
    fn instantiations_of(&self, head: &str) -> Vec<TypeKey> {
        self.covered
            .iter()
            .filter(|key| key.head() == head)
            .cloned()
            .collect()
    }

    fn evidence_name_for(&self, key: &TypeKey) -> Option<String> {
        self.covered
            .contains(key)
            .then(|| format!("verified({key})"))
    }

    fn verifiers_for(&self, key: &TypeKey) -> HashSet<String> {
        if self.covered.contains(key) {
            WITNESSED.iter().map(|v| (*v).to_string()).collect()
        } else {
            HashSet::new()
        }
    }

    fn has_proof_test(&self, key: &TypeKey) -> bool {
        self.tested.contains(key)
    }
}

#[test]
fn any_coverage_source_can_drive_the_expansion() -> miette::Result<()> {
    cordial::init_tracing();
    let resolver = resolver()?;
    let resolve = |text: &str| {
        resolver
            .resolve(text)
            .map_err(|reason| miette::miette!("`{text}`: {reason}"))
    };
    let source = FixedCoverage {
        covered: vec![
            resolve("chrono::DateTime<chrono::Utc>")?,
            resolve("chrono::DateTime<chrono::FixedOffset>")?,
            resolve("chrono::DateTime<chrono::Bare>")?,
        ],
        tested: vec![resolve("chrono::DateTime<chrono::Utc>")?],
    };
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = four_zones();
    let ctx = InstantiationContext::new(&resolver, &source, &skip, &wanted);
    let out =
        expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()?;

    // Covered by the source: Complete. Expected but not covered: Missing.
    for (label, status) in [
        ("chrono::DateTime<chrono::Utc>", AmenableStdStatus::Complete),
        (
            "chrono::DateTime<chrono::FixedOffset>",
            AmenableStdStatus::Complete,
        ),
        (
            "chrono::DateTime<chrono::Local>",
            AmenableStdStatus::Missing,
        ),
        (
            "chrono::DateTime<chrono_tz::Tz>",
            AmenableStdStatus::Missing,
        ),
    ] {
        assert_eq!(find(&out, label)?.status(), status, "{label}");
    }
    assert_eq!(
        find(&out, "chrono::DateTime")?.status(),
        AmenableStdStatus::Partial
    );
    // The source reported one the config did not expect; it is still shown.
    let extra = find(&out, "chrono::DateTime<chrono::Bare>")?;
    assert!(note(extra).contains("not in the expected list"));
    // The source's proof-test knowledge is per instantiation too.
    assert!(find(&out, "chrono::DateTime<chrono::Utc>")?.proof_test());
    assert!(!find(&out, "chrono::DateTime<chrono::FixedOffset>")?.proof_test());
    // The declared bounds come from rustdoc, whatever the source.
    let text = note(find(&out, "chrono::DateTime")?);
    assert!(text.contains("declared bounds `Tz: TimeZone`"), "{text}");
    assert!(
        text.contains("satisfied by Utc, FixedOffset, Local, Tz"),
        "{text}"
    );
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
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), &resolver);
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

#[test]
fn real_chrono_instantiations_are_derived_with_no_configuration() -> miette::Result<()> {
    cordial::init_tracing();
    let (Some(chrono), Some(chrono_tz)) = (cached("chrono"), cached("chrono_tz")) else {
        tracing::info!("skipped: no cached chrono / chrono_tz rustdoc JSON");
        return Ok(());
    };
    let resolver = RustdocTypeResolver::new(vec![chrono, chrono_tz], ResolveCaps::default());
    // No configured list and no coverage source at all: only rustdoc.
    let dump: RegistryDump = serde_json::from_value(json!({
        "evidence_links": [], "proof_records": [], "kani_proofs": [],
    }))
    .into_diagnostic()?;
    let facts = AmenableRegistryEvidence::resolve(&dump, &HashSet::new(), &resolver);
    let skip: VerifierSkipMap = HashMap::new();
    let wanted = ExpectedInstantiations::default();
    let ctx = InstantiationContext::new(&resolver, &facts, &skip, &wanted);
    let out =
        expand_report(&report(vec![parent_entry("chrono::DateTime")?])?, &ctx).into_diagnostic()?;

    let mut children: Vec<&str> = out
        .entries()
        .iter()
        .filter(|entry| entry.parent().is_some())
        .map(|entry| entry.type_path().as_str())
        .collect();
    children.sort_unstable();
    assert_eq!(
        children,
        vec![
            "chrono::DateTime<chrono::FixedOffset>",
            "chrono::DateTime<chrono::Local>",
            "chrono::DateTime<chrono::Utc>",
            "chrono::DateTime<chrono_tz::Tz>",
        ],
        "the implementors of chrono's TimeZone across chrono and chrono_tz, labelled by public path"
    );
    let text = note(find(&out, "chrono::DateTime")?);
    assert!(
        text.contains("derived from the implementors of `Tz: TimeZone`"),
        "{text}"
    );
    Ok(())
}

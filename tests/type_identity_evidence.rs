//! Evidence names resolved to structured keys, including real chrono instantiations.
#![cfg(feature = "amenable_std")]

use cordial::testing::{
    CrateIndex, EvidenceKey, RegistryDump, ResolveCaps, TypeKey, Unresolved, resolve_ext_evidence,
};
use miette::IntoDiagnostic;
use serde_json::{Value, json};

#[path = "support/type_identity_rustdoc_crate.rs"]
mod type_identity_rustdoc_crate;

use type_identity_rustdoc_crate::{Krate, chrono, key, resolver};

fn chrono_tz() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono_tz");
    k.strukt(2, "Tz", &[], &["chrono_tz", "timezones", "Tz"]);
    k.reexport(1, "Tz", 2);
    k.set_items(0, &[1]);
    k.build()
}

// ---- evidence ------------------------------------------------------------

/// The resolved key of an evidence entry, or a test error naming the reason.
fn resolved_key(entry: &EvidenceKey) -> miette::Result<&TypeKey> {
    entry
        .key()
        .as_ref()
        .map_err(|reason| miette::miette!("`{}` did not resolve: {reason}", entry.name()))
}

fn registry(links: &[(&str, &[&str])]) -> miette::Result<RegistryDump> {
    let links: Vec<Value> = links
        .iter()
        .map(|(name, bounds)| json!({"name": name, "basis": "", "index": 0, "bounds": bounds}))
        .collect();
    serde_json::from_value(json!({
        "evidence_links": links, "proof_records": [], "kani_proofs": [],
    }))
    .into_diagnostic()
}

#[test]
fn evidence_names_resolve_to_structured_keys_per_instantiation() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?, chrono_tz()?], ResolveCaps::default());
    let dump = registry(&[
        (
            "amenable_ext::ExtStandard<chrono::DateTime<chrono::Utc>>",
            &[],
        ),
        (
            "amenable_ext::ExtStandard<chrono :: DateTime < chrono_tz :: Tz >>",
            &[],
        ),
        ("amenable_ext::ExtStandard<chrono::offset::Utc>", &[]),
        // Markup from older amenable dumps: not an ext claim, ignored.
        ("amenable_ext::ExtGeneric<chrono::DateTime<Tz>>", &[]),
        ("amenable_std::rust_std::RustStdStandard<String>", &[]),
    ])?;
    let resolved = resolve_ext_evidence(&dump, &r);
    // The std-family link and the generic-wrapper link are not ext claims.
    assert_eq!(resolved.len(), 3, "{resolved:?}");

    let utc = resolved_key(&resolved[0])?;
    let tz = resolved_key(&resolved[1])?;
    assert_ne!(utc, tz, "two instantiations are two claims");
    assert_eq!(utc.head(), tz.head());
    assert_eq!(
        resolved_key(&resolved[2])?,
        &TypeKey::plain("chrono::offset::Utc")
    );
    Ok(())
}

#[test]
fn an_unidentifiable_evidence_type_is_returned_with_its_reason() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    let dump = registry(&[
        ("amenable_ext::ExtStandard<Nonexistent>", &[]),
        (
            "amenable_ext::ExtStandard<chrono::DateTime<chrono::Utc>",
            &[],
        ),
    ])?;
    let resolved = resolve_ext_evidence(&dump, &r);
    assert_eq!(resolved.len(), 2, "nothing is dropped silently");
    assert!(matches!(resolved[0].key(), Err(Unresolved::UnknownName(_))));
    assert!(resolved[1].key().is_err());
    Ok(())
}

// ---- real data -----------------------------------------------------------

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
fn real_chrono_instantiations_resolve_to_four_distinct_keys() -> miette::Result<()> {
    cordial::init_tracing();
    let (Some(chrono), Some(chrono_tz)) = (cached("chrono"), cached("chrono_tz")) else {
        tracing::info!("skipped: no cached chrono / chrono_tz rustdoc JSON");
        return Ok(());
    };
    let r = resolver(vec![chrono, chrono_tz], ResolveCaps::default());
    let keys = [
        key("chrono::DateTime<chrono::Utc>", &r)?,
        key("chrono::DateTime<chrono::FixedOffset>", &r)?,
        key("chrono::DateTime<chrono::Local>", &r)?,
        key("chrono::DateTime<chrono_tz::Tz>", &r)?,
    ];
    for (index, left) in keys.iter().enumerate() {
        for right in &keys[index + 1..] {
            assert_ne!(left, right);
        }
        assert_eq!(left.head(), keys[0].head(), "one generic head");
    }
    // The alias and re-export spellings an evidence name might use.
    assert_eq!(key("chrono::Utc", &r)?, key("Utc", &r)?);
    // chrono's own alias expands to its real target.
    assert_eq!(
        key("chrono::Duration", &r)?,
        key("chrono::TimeDelta", &r)?,
        "a type alias resolves to the type it names"
    );
    Ok(())
}

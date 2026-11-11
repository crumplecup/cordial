//! Rustdoc name lookup: modules, re-exports, spellings, instantiations, and unresolved input.
#![cfg(feature = "amenable_std")]

use cordial::testing::{CrateIndex, Lookup, ResolveCaps, TypeResolver, Unresolved};

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

impl Krate {
    /// A `pub use target::*` item.
    fn glob(&mut self, id: u32, target: u32) {
        self.item(
            id,
            None,
            serde_json::json!({"use": {"source": "x", "name": "x", "id": target, "is_glob": true}}),
        );
    }
}

// ---- lookup --------------------------------------------------------------

#[test]
fn lookup_follows_modules_and_reexports() -> miette::Result<()> {
    cordial::init_tracing();
    let index = chrono()?;
    assert!(matches!(index.lookup(&["offset", "Utc"]), Lookup::Found(_)));
    assert!(matches!(index.lookup(&["Utc"]), Lookup::Found(_)));
    assert_eq!(index.lookup(&["Nope"]), Lookup::Missing);
    // Both spellings are the same item.
    assert_eq!(index.lookup(&["offset", "Utc"]), index.lookup(&["Utc"]));
    Ok(())
}

#[test]
fn glob_reexports_are_followed_and_cycles_terminate() -> miette::Result<()> {
    cordial::init_tracing();
    let mut k = Krate::new("g");
    k.module(1, "a", &[10, 4], &["g", "a"]);
    k.module(2, "b", &[11, 5], &["g", "b"]);
    k.strukt(3, "Hidden", &[], &["g", "b", "Hidden"]);
    // a globs b, b globs a: a cycle.
    k.glob(10, 2);
    k.glob(11, 1);
    k.set_items(2, &[11, 3]);
    k.set_items(0, &[1, 2]);
    // `a` also has a non-glob child so the module is not empty.
    k.strukt(4, "Local", &[], &["g", "a", "Local"]);
    let index = k.build()?;
    assert!(matches!(index.lookup(&["a", "Hidden"]), Lookup::Found(_)));
    assert_eq!(index.lookup(&["a", "Absent"]), Lookup::Missing);
    Ok(())
}

// ---- resolution ----------------------------------------------------------

#[test]
fn every_spelling_of_a_type_resolves_to_one_key() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    let qualified = key("chrono::Utc", &r)?;
    let defining = key("chrono::offset::Utc", &r)?;
    let bare = key("Utc", &r)?;
    assert_eq!(qualified, defining);
    assert_eq!(qualified, bare);
    assert_eq!(qualified.head(), "chrono::offset::Utc");
    Ok(())
}

#[test]
fn generic_instantiations_get_distinct_keys_and_ignore_spacing() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?, chrono_tz()?], ResolveCaps::default());
    let utc = key("chrono::DateTime<chrono::Utc>", &r)?;
    let tz = key("chrono::DateTime<chrono_tz::Tz>", &r)?;
    assert_ne!(utc, tz);
    assert_eq!(utc.head(), tz.head());
    assert_eq!(utc.args().len(), 1);
    assert_eq!(
        utc.to_string(),
        "chrono::datetime::DateTime<chrono::offset::Utc>"
    );
    assert_eq!(
        utc,
        key("chrono :: DateTime < chrono :: Utc >", &r)?,
        "stringify spacing must not change identity"
    );
    assert_eq!(utc.node_count(), 2);
    Ok(())
}

#[test]
fn a_type_alias_expands_with_its_arguments_substituted() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    let expanded = key("chrono::ParseResult<chrono::Utc>", &r)?;
    // `Result` lives in `core`, which is not allowlisted: opaque, kept as
    // rustdoc spells its defining path, with both arguments resolved.
    assert_eq!(expanded.head(), "core::result::Result");
    assert_eq!(expanded.args().len(), 2);
    assert_eq!(expanded.args()[0].head(), "chrono::offset::Utc");
    assert_eq!(expanded.args()[1].head(), "chrono::format::ParseError");
    Ok(())
}

#[test]
fn head_only_resolution_ignores_unresolvable_type_parameters() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    // `Tz` is a type parameter, not a type.
    assert!(matches!(
        r.resolve("chrono::DateTime<Tz>"),
        Err(Unresolved::UnknownName(name)) if name == "Tz"
    ));
    let head = r
        .resolve_head("chrono::DateTime<Tz>")
        .map_err(|reason| miette::miette!("head did not resolve: {reason}"))?;
    assert_eq!(head.head(), "chrono::datetime::DateTime");
    assert!(head.args().is_empty());
    Ok(())
}

#[test]
fn paths_under_crates_outside_the_allowlist_are_opaque() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    let opaque = key("elsewhere::Thing<chrono::Utc>", &r)?;
    assert_eq!(opaque.head(), "elsewhere::Thing");
    assert_eq!(opaque.args()[0].head(), "chrono::offset::Utc");
    // Non-path forms are opaque too.
    assert_eq!(key("&str", &r)?.head(), "&str");
    Ok(())
}

#[test]
fn unresolvable_input_is_a_named_reason_never_a_silent_miss() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::default());
    assert!(matches!(
        r.resolve("Nothing"),
        Err(Unresolved::UnknownName(_))
    ));
    assert!(matches!(
        r.resolve("chrono::offset"),
        Err(Unresolved::NotAType(_))
    ));
    assert!(matches!(
        r.resolve("chrono::DateTime<chrono::Utc"),
        Err(Unresolved::Malformed(_))
    ));
    let message = match r.resolve("Nothing") {
        Err(reason) => reason.to_string(),
        Ok(found) => return Err(miette::miette!("`Nothing` resolved to {found}")),
    };
    assert!(message.contains("Nothing"), "{message}");
    Ok(())
}

#[test]
fn a_bare_name_in_two_allowlisted_crates_is_ambiguous() -> miette::Result<()> {
    cordial::init_tracing();
    let mut other = Krate::new("other");
    other.strukt(2, "Tz", &[], &["other", "Tz"]);
    other.reexport(1, "Tz", 2);
    other.set_items(0, &[1]);
    let r = resolver(vec![chrono_tz()?, other.build()?], ResolveCaps::default());
    match r.resolve("Tz") {
        Err(Unresolved::Ambiguous { name, candidates }) => {
            assert_eq!(name, "Tz");
            assert_eq!(candidates.len(), 2, "{candidates:?}");
        }
        other => return Err(miette::miette!("expected ambiguity, got {other:?}")),
    }
    // Qualified, it is unambiguous.
    assert!(key("chrono_tz::Tz", &r).is_ok());
    Ok(())
}

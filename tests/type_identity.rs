#![cfg(feature = "amenable_std")]

//! Structured type identity: parsing, rustdoc name lookup, alias expansion,
//! and the caps that keep resolution bounded.

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{
    CrateIndex, EvidenceKey, EvidenceKind, Lookup, RegistryDump, ResolveCaps, RustdocTypeResolver,
    TypeKey, TypeResolver, Unresolved, normalize_type_text, parse_type_text, resolve_ext_evidence,
};

// ---- a tiny rustdoc crate builder --------------------------------------

/// Build a minimal, valid rustdoc JSON crate named `name`. Ids are chosen by
/// the caller so cross-references read plainly in the tests.
struct Krate {
    name: String,
    root: u32,
    index: serde_json::Map<String, Value>,
    paths: serde_json::Map<String, Value>,
}

impl Krate {
    fn new(name: &str) -> Self {
        let mut krate = Self {
            name: name.to_string(),
            root: 0,
            index: serde_json::Map::new(),
            paths: serde_json::Map::new(),
        };
        krate.module(0, name, &[], &[name]);
        krate
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

    fn module(&mut self, id: u32, name: &str, items: &[u32], path: &[&str]) {
        self.item(
            id,
            Some(name),
            json!({"module": {"is_crate": id == 0, "items": items, "is_stripped": false}}),
        );
        self.path(id, 0, path, "module");
    }

    fn set_items(&mut self, module: u32, items: &[u32]) {
        if let Some(slot) = self
            .index
            .get_mut(&module.to_string())
            .and_then(|item| item.pointer_mut("/inner/module/items"))
        {
            *slot = json!(items);
        }
    }

    fn strukt(&mut self, id: u32, name: &str, params: &[&str], path: &[&str]) {
        let params: Vec<Value> = params.iter().map(|p| type_param(p)).collect();
        self.item(
            id,
            Some(name),
            json!({"struct": {
                "kind": {"plain": {"fields": [], "has_stripped_fields": false}},
                "generics": {"params": params, "where_predicates": []},
                "impls": [],
            }}),
        );
        self.path(id, 0, path, "struct");
    }

    fn reexport(&mut self, id: u32, name: &str, target: u32) {
        self.item(
            id,
            None,
            json!({"use": {"source": name, "name": name, "id": target, "is_glob": false}}),
        );
    }

    fn glob(&mut self, id: u32, target: u32) {
        self.item(
            id,
            None,
            json!({"use": {"source": "x", "name": "x", "id": target, "is_glob": true}}),
        );
    }

    fn alias(&mut self, id: u32, name: &str, params: &[&str], ty: Value, path: &[&str]) {
        let params: Vec<Value> = params.iter().map(|p| type_param(p)).collect();
        self.item(
            id,
            Some(name),
            json!({"type_alias": {
                "type": ty,
                "generics": {"params": params, "where_predicates": []},
            }}),
        );
        self.path(id, 0, path, "type_alias");
    }

    fn external(&mut self, id: u32, crate_id: u32, path: &[&str], kind: &str) {
        self.path(id, crate_id, path, kind);
    }

    fn build(self) -> miette::Result<CrateIndex> {
        let document = json!({
            "root": self.root, "crate_version": "0.0.0", "includes_private": false,
            "index": self.index, "paths": self.paths, "external_crates": {},
            "target": {"triple": "x86_64-unknown-linux-gnu", "target_features": []},
            "format_version": 57,
        });
        let krate = serde_json::from_value(document).into_diagnostic()?;
        Ok(CrateIndex::new(&self.name, krate))
    }
}

fn type_param(name: &str) -> Value {
    json!({"name": name, "kind": {"type": {"bounds": [], "default": null, "is_synthetic": false}}})
}

fn path_ty(path: &str, id: u32, args: &[Value]) -> Value {
    let args: Vec<Value> = args.iter().map(|a| json!({"type": a})).collect();
    json!({"resolved_path": {
        "path": path, "id": id,
        "args": if args.is_empty() { Value::Null }
                else { json!({"angle_bracketed": {"args": args, "constraints": []}}) },
    }})
}

fn generic(name: &str) -> Value {
    json!({"generic": name})
}

/// chrono-shaped: `Utc` defined in `offset` and re-exported at the root,
/// `DateTime<Tz>` defined in a private module and re-exported, and an alias
/// `ParseResult<T> = Result<T, ParseError>` over a std item.
fn chrono() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono");
    k.module(3, "offset", &[2], &["chrono", "offset"]);
    k.strukt(2, "Utc", &[], &["chrono", "offset", "Utc"]);
    k.reexport(1, "Utc", 2);
    k.module(4, "datetime", &[11], &["chrono", "datetime"]);
    k.strukt(11, "DateTime", &["Tz"], &["chrono", "datetime", "DateTime"]);
    k.reexport(12, "DateTime", 11);
    k.strukt(9, "ParseError", &[], &["chrono", "format", "ParseError"]);
    k.external(20, 1, &["core", "result", "Result"], "enum");
    k.alias(
        8,
        "ParseResult",
        &["T"],
        path_ty("Result", 20, &[generic("T"), path_ty("ParseError", 9, &[])]),
        &["chrono", "ParseResult"],
    );
    k.set_items(0, &[1, 3, 4, 12, 8, 9]);
    k.build()
}

fn chrono_tz() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono_tz");
    k.strukt(2, "Tz", &[], &["chrono_tz", "timezones", "Tz"]);
    k.reexport(1, "Tz", 2);
    k.set_items(0, &[1]);
    k.build()
}

fn resolver(crates: Vec<CrateIndex>, caps: ResolveCaps) -> RustdocTypeResolver {
    RustdocTypeResolver::new(crates, caps)
}

/// Resolve, reporting an unresolved type as a test error.
fn key(text: &str, r: &RustdocTypeResolver) -> miette::Result<TypeKey> {
    r.resolve(text)
        .map_err(|reason| miette::miette!("`{text}` did not resolve: {reason}"))
}

fn some<T>(value: Option<T>, what: &str) -> miette::Result<T> {
    value.ok_or_else(|| miette::miette!("{what}"))
}

// ---- text ----------------------------------------------------------------

#[test]
fn normalization_removes_stringify_spacing_but_keeps_word_boundaries() {
    cordial::init_tracing();
    assert_eq!(
        normalize_type_text("chrono :: DateTime < chrono :: Utc >"),
        "chrono::DateTime<chrono::Utc>"
    );
    assert_eq!(normalize_type_text("&  mut   Foo"), "&mut Foo");
    assert_eq!(normalize_type_text("dyn   Trait"), "dyn Trait");
}

#[test]
fn parsing_reads_nested_arguments_and_drops_lifetimes() -> miette::Result<()> {
    cordial::init_tracing();
    let parsed = some(parse_type_text("Cow<'a,Map<K,Vec<V>>>"), "did not parse")?;
    assert_eq!(parsed.path(), "Cow");
    assert_eq!(parsed.args().len(), 1);
    let map = &parsed.args()[0];
    assert_eq!(map.path(), "Map");
    assert_eq!(map.args().len(), 2);
    assert_eq!(map.args()[1].path(), "Vec");
    assert_eq!(map.args()[1].args()[0].path(), "V");
    Ok(())
}

#[test]
fn non_path_types_are_opaque_and_unbalanced_text_is_rejected() -> miette::Result<()> {
    cordial::init_tracing();
    for text in ["&str", "(A,B)", "[u8;4]", "dyn Fn(A)->B", "*const T"] {
        let parsed = some(parse_type_text(text), text)?;
        assert_eq!(parsed.path(), text);
        assert!(parsed.args().is_empty());
    }
    assert!(parse_type_text("Foo<Bar").is_none());
    assert!(parse_type_text("").is_none());
    // `->` inside a generic argument is not a closing bracket.
    let nested = some(parse_type_text("Box<fn(A)->B>"), "did not parse")?;
    assert_eq!(nested.args().len(), 1);
    Ok(())
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
    // `Tz` is a type parameter in `ExtGeneric<DateTime<Tz>>`, not a type.
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

// ---- caps ----------------------------------------------------------------

/// `Chain0 = Chain1 = ... = ChainN = Leaf`.
fn alias_chain(length: u32) -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chain");
    k.strukt(500, "Leaf", &[], &["chain", "Leaf"]);
    let mut items = vec![500];
    for step in 0..length {
        let id = 100 + step;
        let target = if step + 1 == length {
            500
        } else {
            100 + step + 1
        };
        let name = format!("Chain{step}");
        k.alias(
            id,
            &name,
            &[],
            path_ty("next", target, &[]),
            &["chain", &name],
        );
        items.push(id);
    }
    k.set_items(0, &items);
    k.build()
}

#[test]
fn alias_chains_are_followed_up_to_the_depth_cap() -> miette::Result<()> {
    cordial::init_tracing();
    let roomy = resolver(vec![alias_chain(3)?], ResolveCaps::new(8, 64));
    assert_eq!(key("chain::Chain0", &roomy)?.head(), "chain::Leaf");

    let tight = resolver(vec![alias_chain(3)?], ResolveCaps::new(2, 64));
    match tight.resolve("chain::Chain0") {
        Err(Unresolved::AliasDepthExceeded(name)) => assert!(name.starts_with("Chain")),
        other => {
            return Err(miette::miette!(
                "expected the depth cap to trip, got {other:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn an_expansion_larger_than_the_node_cap_is_rejected() -> miette::Result<()> {
    cordial::init_tracing();
    let r = resolver(vec![chrono()?], ResolveCaps::new(8, 2));
    // DateTime<Utc> is 2 nodes; ParseResult<Utc> expands to 3.
    assert!(r.resolve("chrono::DateTime<chrono::Utc>").is_ok());
    assert!(matches!(
        r.resolve("chrono::ParseResult<chrono::Utc>"),
        Err(Unresolved::TypeTooLarge(_))
    ));
    Ok(())
}

#[test]
fn default_caps_match_the_documented_values() {
    cordial::init_tracing();
    let caps = ResolveCaps::default();
    assert_eq!(caps.alias_depth(), 8);
    assert_eq!(caps.max_type_nodes(), 64);
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
        (
            "amenable_ext::ExtGeneric<chrono::DateTime<Tz>>",
            &["chrono::offset::TimeZone"],
        ),
        ("amenable_std::rust_std::RustStdStandard<String>", &[]),
    ])?;
    let resolved = resolve_ext_evidence(&dump, &r);
    // The std-family link is not an ext claim.
    assert_eq!(resolved.len(), 4, "{resolved:?}");

    let utc = resolved_key(&resolved[0])?;
    let tz = resolved_key(&resolved[1])?;
    assert_eq!(resolved[0].kind(), EvidenceKind::Concrete);
    assert_ne!(utc, tz, "two instantiations are two claims");
    assert_eq!(utc.head(), tz.head());
    assert_eq!(
        resolved_key(&resolved[2])?,
        &TypeKey::plain("chrono::offset::Utc")
    );

    let generic = &resolved[3];
    assert_eq!(generic.kind(), EvidenceKind::Generic);
    assert_eq!(generic.bounds(), &["chrono::offset::TimeZone".to_string()]);
    let head = resolved_key(generic)?;
    assert_eq!(
        head.head(),
        utc.head(),
        "the generic claim is about the same head"
    );
    assert!(head.args().is_empty());
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

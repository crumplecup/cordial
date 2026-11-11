//! A tiny rustdoc crate builder for the type-identity tests.
#![cfg(feature = "amenable_std")]

use cordial::testing::{CrateIndex, ResolveCaps, RustdocTypeResolver, TypeKey, TypeResolver};
use miette::IntoDiagnostic;
use serde_json::{Value, json};

// ---- a tiny rustdoc crate builder --------------------------------------

/// Build a minimal, valid rustdoc JSON crate named `name`. Ids are chosen by
/// the caller so cross-references read plainly in the tests.
pub struct Krate {
    pub name: String,
    pub root: u32,
    pub index: serde_json::Map<String, Value>,
    pub paths: serde_json::Map<String, Value>,
}

impl Krate {
    pub fn new(name: &str) -> Self {
        let mut krate = Self {
            name: name.to_string(),
            root: 0,
            index: serde_json::Map::new(),
            paths: serde_json::Map::new(),
        };
        krate.module(0, name, &[], &[name]);
        krate
    }

    pub fn item(&mut self, id: u32, name: Option<&str>, inner: Value) {
        self.index.insert(
            id.to_string(),
            json!({
                "id": id, "crate_id": 0, "name": name, "span": null,
                "visibility": "public", "docs": null, "links": {}, "attrs": [],
                "deprecation": null, "inner": inner,
            }),
        );
    }

    pub fn path(&mut self, id: u32, crate_id: u32, path: &[&str], kind: &str) {
        self.paths.insert(
            id.to_string(),
            json!({"crate_id": crate_id, "path": path, "kind": kind}),
        );
    }

    pub fn module(&mut self, id: u32, name: &str, items: &[u32], path: &[&str]) {
        self.item(
            id,
            Some(name),
            json!({"module": {"is_crate": id == 0, "items": items, "is_stripped": false}}),
        );
        self.path(id, 0, path, "module");
    }

    pub fn set_items(&mut self, module: u32, items: &[u32]) {
        if let Some(slot) = self
            .index
            .get_mut(&module.to_string())
            .and_then(|item| item.pointer_mut("/inner/module/items"))
        {
            *slot = json!(items);
        }
    }

    pub fn strukt(&mut self, id: u32, name: &str, params: &[&str], path: &[&str]) {
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

    pub fn reexport(&mut self, id: u32, name: &str, target: u32) {
        self.item(
            id,
            None,
            json!({"use": {"source": name, "name": name, "id": target, "is_glob": false}}),
        );
    }

    pub fn alias(&mut self, id: u32, name: &str, params: &[&str], ty: Value, path: &[&str]) {
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

    pub fn external(&mut self, id: u32, crate_id: u32, path: &[&str], kind: &str) {
        self.path(id, crate_id, path, kind);
    }

    pub fn build(self) -> miette::Result<CrateIndex> {
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

pub fn type_param(name: &str) -> Value {
    json!({"name": name, "kind": {"type": {"bounds": [], "default": null, "is_synthetic": false}}})
}

pub fn path_ty(path: &str, id: u32, args: &[Value]) -> Value {
    let args: Vec<Value> = args.iter().map(|a| json!({"type": a})).collect();
    json!({"resolved_path": {
        "path": path, "id": id,
        "args": if args.is_empty() { Value::Null }
                else { json!({"angle_bracketed": {"args": args, "constraints": []}}) },
    }})
}

pub fn generic(name: &str) -> Value {
    json!({"generic": name})
}

/// chrono-shaped: `Utc` defined in `offset` and re-exported at the root,
/// `DateTime<Tz>` defined in a private module and re-exported, and an alias
/// `ParseResult<T> = Result<T, ParseError>` over a std item.
pub fn chrono() -> miette::Result<CrateIndex> {
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

pub fn resolver(crates: Vec<CrateIndex>, caps: ResolveCaps) -> RustdocTypeResolver {
    RustdocTypeResolver::new(crates, caps)
}

/// Resolve, reporting an unresolved type as a test error.
pub fn key(text: &str, r: &RustdocTypeResolver) -> miette::Result<TypeKey> {
    r.resolve(text)
        .map_err(|reason| miette::miette!("`{text}` did not resolve: {reason}"))
}

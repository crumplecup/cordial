//! A tiny rustdoc crate builder, the chrono-shaped crates built with it, and a resolver over them.
#![cfg(feature = "amenable_std")]

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use cordial::testing::{CrateIndex, ResolveCaps, RustdocTypeResolver};

pub struct Krate {
    pub name: String,
    pub index: serde_json::Map<String, Value>,
    pub paths: serde_json::Map<String, Value>,
    pub root_items: Vec<u32>,
}

impl Krate {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            index: serde_json::Map::new(),
            paths: serde_json::Map::new(),
            root_items: Vec::new(),
        }
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

    pub fn generics(params: &[&str]) -> Value {
        let params: Vec<Value> = params
            .iter()
            .map(|p| {
                json!({"name": p, "kind": {"type": {"bounds": [], "default": null, "is_synthetic": false}}})
            })
            .collect();
        json!({"params": params, "where_predicates": []})
    }

    /// A struct in the crate root, with the given trait-impl item ids.
    pub fn strukt(&mut self, id: u32, name: &str, params: &[&str], impls: &[u32]) {
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

    /// Declare `param: trait_name` on the struct `id`, inline
    /// (`struct S<P: Trait>`) or, if `in_where`, in a `where` clause.
    pub fn declare_bound(
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

    pub fn enumeration(&mut self, id: u32, name: &str, impls: &[u32]) {
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

    pub fn trait_item(&mut self, id: u32, name: &str) {
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
    pub fn impl_of(
        &mut self,
        id: u32,
        trait_id: u32,
        trait_name: &str,
        type_id: u32,
        type_name: &str,
    ) {
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

    pub fn build(mut self) -> miette::Result<CrateIndex> {
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
pub fn chrono() -> miette::Result<CrateIndex> {
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
pub fn chrono_tz() -> miette::Result<CrateIndex> {
    let mut k = Krate::new("chrono_tz");
    k.enumeration(2, "Tz", &[20]);
    k.path(7, 1, &["chrono", "TimeZone"], "trait");
    k.impl_of(20, 7, "TimeZone", 2, "Tz");
    k.build()
}

pub fn resolver() -> miette::Result<RustdocTypeResolver> {
    Ok(RustdocTypeResolver::new(
        vec![chrono()?, chrono_tz()?],
        ResolveCaps::default(),
    ))
}

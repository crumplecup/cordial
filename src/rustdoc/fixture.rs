//! Synthetic rustdoc [`Crate`] values for integration tests.

use std::collections::HashMap;
use std::path::Path;

use rustdoc_types::{
    Abi, Crate, Function, FunctionHeader, FunctionSignature, Generics, Id, Impl, Item, ItemEnum,
    ItemKind, ItemSummary, Module, Path as RustdocPath, Struct, StructKind, Target, TargetFeature,
    Type, Visibility,
};

use crate::error::CordialResult;

use tracing::instrument;
#[instrument(level = "info", skip(path), err(level = "warn"))]
pub fn write_rustdoc_crate_json(path: &Path, krate: &Crate) -> CordialResult<()> {
    std::fs::write(path, serde_json::to_string_pretty(krate)?)?;
    Ok(())
}

/// Minimal crate with one struct missing `Deserialize`.
#[instrument(level = "debug")]
pub fn demo_impl_coverage_crate() -> Crate {
    let root = Id(0);
    let widget = Id(1);
    let serialize_impl = Id(2);
    let draw_impl = Id(3);
    let draw_fn = Id(4);

    let mut index = HashMap::new();
    index.insert(
        root,
        module_item(root, "demo", true, vec![widget, serialize_impl, draw_impl]),
    );
    index.insert(
        widget,
        struct_item(widget, "Widget", vec![serialize_impl, draw_impl]),
    );
    index.insert(
        serialize_impl,
        Item {
            id: serialize_impl,
            crate_id: 0,
            name: None,
            span: None,
            visibility: Visibility::Default,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            inner: ItemEnum::Impl(Impl {
                is_unsafe: false,
                generics: empty_generics(),
                provided_trait_methods: Vec::new(),
                trait_: Some(RustdocPath {
                    path: "serde::Serialize".to_string(),
                    id: Id(10),
                    args: None,
                }),
                for_: Type::ResolvedPath(RustdocPath {
                    path: "demo::Widget".to_string(),
                    id: widget,
                    args: None,
                }),
                items: Vec::new(),
                is_negative: false,
                is_synthetic: false,
                blanket_impl: None,
            }),
        },
    );
    index.insert(
        draw_impl,
        Item {
            id: draw_impl,
            crate_id: 0,
            name: None,
            span: None,
            visibility: Visibility::Default,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            inner: ItemEnum::Impl(Impl {
                is_unsafe: false,
                generics: empty_generics(),
                provided_trait_methods: Vec::new(),
                trait_: None,
                for_: Type::ResolvedPath(RustdocPath {
                    path: "demo::Widget".to_string(),
                    id: widget,
                    args: None,
                }),
                items: vec![draw_fn],
                is_negative: false,
                is_synthetic: false,
                blanket_impl: None,
            }),
        },
    );
    index.insert(
        draw_fn,
        Item {
            id: draw_fn,
            crate_id: 0,
            name: Some("draw".to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            inner: ItemEnum::Function(empty_function()),
        },
    );

    let mut paths = HashMap::new();
    paths.insert(root, summary(vec!["demo".to_string()], ItemKind::Module));
    paths.insert(
        widget,
        summary(
            vec!["demo".to_string(), "Widget".to_string()],
            ItemKind::Struct,
        ),
    );

    base_crate(root, paths, index)
}

#[instrument(level = "debug")]
pub fn demo_trenchcoat_crate() -> Crate {
    let root = Id(0);
    let foreign = Id(1);
    let wrapper = Id(2);
    let bare_foreign = Id(4);
    let from_impl = Id(3);

    let mut index = HashMap::new();
    index.insert(
        root,
        module_item(
            root,
            "demo",
            true,
            vec![foreign, wrapper, bare_foreign, from_impl],
        ),
    );
    index.insert(foreign, struct_item(foreign, "Foreign", vec![]));
    index.insert(
        wrapper,
        struct_item(wrapper, "ForeignWrapper", vec![from_impl]),
    );
    index.insert(
        bare_foreign,
        struct_item(bare_foreign, "BareForeign", vec![]),
    );
    index.insert(
        from_impl,
        Item {
            id: from_impl,
            crate_id: 0,
            name: None,
            span: None,
            visibility: Visibility::Default,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            inner: ItemEnum::Impl(Impl {
                is_unsafe: false,
                generics: empty_generics(),
                provided_trait_methods: Vec::new(),
                trait_: Some(RustdocPath {
                    path: "core::convert::From".to_string(),
                    id: Id(99),
                    args: Some(Box::new(rustdoc_types::GenericArgs::AngleBracketed {
                        args: vec![rustdoc_types::GenericArg::Type(Type::ResolvedPath(
                            RustdocPath {
                                path: "demo::Foreign".to_string(),
                                id: foreign,
                                args: None,
                            },
                        ))],
                        constraints: Vec::new(),
                    })),
                }),
                for_: Type::ResolvedPath(RustdocPath {
                    path: "demo::ForeignWrapper".to_string(),
                    id: wrapper,
                    args: None,
                }),
                items: Vec::new(),
                is_negative: false,
                is_synthetic: false,
                blanket_impl: None,
            }),
        },
    );

    let mut paths = HashMap::new();
    paths.insert(root, summary(vec!["demo".to_string()], ItemKind::Module));
    paths.insert(
        foreign,
        summary(
            vec!["demo".to_string(), "Foreign".to_string()],
            ItemKind::Struct,
        ),
    );
    paths.insert(
        wrapper,
        summary(
            vec!["demo".to_string(), "ForeignWrapper".to_string()],
            ItemKind::Struct,
        ),
    );
    paths.insert(
        bare_foreign,
        summary(
            vec!["demo".to_string(), "BareForeign".to_string()],
            ItemKind::Struct,
        ),
    );

    base_crate(root, paths, index)
}

#[instrument(level = "debug")]
pub fn demo_shadow_crate() -> Crate {
    let root = Id(0);
    let widget = Id(1);
    let shadow_widget = Id(2);

    let mut index = HashMap::new();
    index.insert(
        root,
        module_item(root, "demo", true, vec![widget, shadow_widget]),
    );
    index.insert(widget, struct_item(widget, "Widget", vec![]));
    index.insert(
        shadow_widget,
        struct_item(shadow_widget, "WidgetShadow", vec![]),
    );

    let mut paths = HashMap::new();
    paths.insert(root, summary(vec!["demo".to_string()], ItemKind::Module));
    paths.insert(
        widget,
        summary(
            vec!["demo".to_string(), "Widget".to_string()],
            ItemKind::Struct,
        ),
    );
    paths.insert(
        shadow_widget,
        summary(
            vec!["demo".to_string(), "WidgetShadow".to_string()],
            ItemKind::Struct,
        ),
    );

    base_crate(root, paths, index)
}

fn module_item(id: Id, name: &str, is_crate: bool, items: Vec<Id>) -> Item {
    Item {
        id,
        crate_id: 0,
        name: Some(name.to_string()),
        span: None,
        visibility: Visibility::Public,
        docs: None,
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        inner: ItemEnum::Module(Module {
            is_crate,
            items,
            is_stripped: false,
        }),
    }
}

fn struct_item(id: Id, name: &str, impls: Vec<Id>) -> Item {
    Item {
        id,
        crate_id: 0,
        name: Some(name.to_string()),
        span: None,
        visibility: Visibility::Public,
        docs: None,
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        inner: ItemEnum::Struct(Struct {
            kind: StructKind::Unit,
            generics: empty_generics(),
            impls,
        }),
    }
}

fn summary(path: Vec<String>, kind: ItemKind) -> ItemSummary {
    ItemSummary {
        crate_id: 0,
        path,
        kind,
    }
}

fn empty_generics() -> Generics {
    Generics {
        params: Vec::new(),
        where_predicates: Vec::new(),
    }
}

fn empty_function() -> Function {
    Function {
        sig: FunctionSignature {
            inputs: Vec::new(),
            output: None,
            is_c_variadic: false,
        },
        generics: empty_generics(),
        header: FunctionHeader {
            is_const: false,
            is_unsafe: false,
            is_async: false,
            abi: Abi::Rust,
        },
        has_body: true,
    }
}

fn base_crate(root: Id, paths: HashMap<Id, ItemSummary>, index: HashMap<Id, Item>) -> Crate {
    Crate {
        root,
        crate_version: Some("0.1.0".to_string()),
        includes_private: false,
        index,
        paths,
        external_crates: HashMap::new(),
        target: Target {
            triple: "x86_64-unknown-linux-gnu".to_string(),
            target_features: Vec::<TargetFeature>::new(),
        },
        format_version: 40,
    }
}

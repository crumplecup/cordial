//! Seed synthetic rustdoc for the minimal-workspace impl coverage fixture.

use miette::{IntoDiagnostic, WrapErr};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rustdoc_types::{
    Crate, Generics, Id, Item, ItemEnum, ItemKind, ItemSummary, Module, Struct, StructKind, Target,
    Visibility,
};

fn write_minimal_rustdoc_file(
    path: &Path,
    crate_name: &str,
    type_name: &str,
) -> miette::Result<()> {
    let root_id = Id(1);
    let struct_id = Id(2);
    let crate_key = crate_name.replace('-', "_");

    let mut index = HashMap::new();
    index.insert(
        root_id,
        Item {
            id: root_id,
            crate_id: 0,
            name: Some(crate_key.clone()),
            span: None,
            visibility: Visibility::Public,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            stability: None,
            const_stability: None,
            inner: ItemEnum::Module(Module {
                is_crate: true,
                items: vec![struct_id],
                is_stripped: false,
            }),
        },
    );
    index.insert(
        struct_id,
        Item {
            id: struct_id,
            crate_id: 0,
            name: Some(type_name.to_string()),
            span: None,
            visibility: Visibility::Public,
            docs: None,
            links: HashMap::new(),
            attrs: Vec::new(),
            deprecation: None,
            stability: None,
            const_stability: None,
            inner: ItemEnum::Struct(Struct {
                kind: StructKind::Unit,
                impls: Vec::new(),
                generics: Generics {
                    params: Vec::new(),
                    where_predicates: Vec::new(),
                },
            }),
        },
    );

    let mut paths = HashMap::new();
    paths.insert(
        root_id,
        ItemSummary {
            crate_id: 0,
            path: vec![crate_key.clone()],
            kind: ItemKind::Module,
        },
    );
    paths.insert(
        struct_id,
        ItemSummary {
            crate_id: 0,
            path: vec![crate_key, type_name.to_string()],
            kind: ItemKind::Struct,
        },
    );

    let krate = Crate {
        root: root_id,
        crate_version: Some("0.1.0".to_string()),
        includes_private: false,
        index,
        paths,
        external_crates: HashMap::new(),
        target: Target {
            triple: "x86_64-unknown-linux-gnu".to_string(),
            target_features: Vec::new(),
        },
        format_version: rustdoc_types::FORMAT_VERSION,
    };

    let body = serde_json::to_string_pretty(&krate)
        .into_diagnostic()
        .wrap_err("serialize rustdoc")?;
    fs::write(path, body)
        .into_diagnostic()
        .wrap_err("write rustdoc json")?;
    Ok(())
}

/// Seed the same rustdoc inputs used by elicit_doc's pipeline fixture.
pub fn seed_minimal_impl_fixture(store_root: &Path) -> miette::Result<()> {
    write_store_rustdoc(store_root, "elicitation", "Handle")?;
    write_store_rustdoc(store_root, "url", "Widget")?;
    Ok(())
}

fn write_store_rustdoc(store_root: &Path, crate_name: &str, type_name: &str) -> miette::Result<()> {
    let cache = store_root.join("cache/rustdoc");
    fs::create_dir_all(&cache)
        .into_diagnostic()
        .wrap_err("store rustdoc dir")?;
    write_minimal_rustdoc_file(
        &cache.join(format!("{crate_name}.json")),
        crate_name,
        type_name,
    )
}

pub fn run_cordial_impl_coverage(
    workspace: &Path,
    store_root: &Path,
    crate_name: Option<&str>,
) -> miette::Result<()> {
    use cordial::{IMPL_COVERAGE_ETIQUETTE, NamedRunFilter, Session, SessionBuilder};

    seed_minimal_impl_fixture(store_root)?;

    let session = SessionBuilder::new(workspace)
        .with_store_root(store_root)
        .register(&IMPL_COVERAGE_ETIQUETTE)
        .build();

    let filter = match crate_name {
        Some(name) => NamedRunFilter::etiquettes(["impl-coverage"]).with_crate(name.to_string()),
        None => NamedRunFilter::etiquettes(["impl-coverage"]),
    };
    session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("cordial impl coverage run")?;
    Ok(())
}

//! Seed rustdoc for minimal-workspace shadow mirror tests.
//!
//! Self-contained (own copy of `write_minimal_rustdoc`/`write_minimal_rustdoc_file`,
//! not shared with `minimal_fixture.rs`): each `#[path]`-included consumer
//! compiles only the fixture module(s) it actually calls, so this doesn't
//! carry a shared re-export surface that leaves some consumers' unused
//! half flagged as dead code. `shadow_coverage.rs` holds the coverage-
//! running helpers built on top of this file's fixtures, kept separate
//! since not every consumer of `seed_minimal_shadow_fixture` needs them.

use miette::{IntoDiagnostic, WrapErr};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rustdoc_types::{
    Crate, Generics, Id, Item, ItemEnum, ItemKind, ItemSummary, Module, Struct, StructKind, Target,
    Visibility,
};

/// Write a public unit struct into `{workspace}/target/doc/{crate}.json`.
pub fn write_minimal_rustdoc(
    workspace: &Path,
    crate_name: &str,
    type_name: &str,
) -> miette::Result<PathBuf> {
    let doc_dir = workspace.join("target/doc");
    fs::create_dir_all(&doc_dir)
        .into_diagnostic()
        .wrap_err("doc dir")?;
    let path = doc_dir.join(format!("{crate_name}.json"));
    write_minimal_rustdoc_file(&path, crate_name, type_name)?;
    Ok(path)
}

pub fn write_minimal_rustdoc_file(path: &Path, crate_name: &str, type_name: &str) -> miette::Result<()> {
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

/// Seed upstream and shadow rustdoc for the url ↔ elicit_url pair.
pub fn seed_minimal_shadow_fixture(workspace: &Path, store_root: &Path) -> miette::Result<()> {
    write_minimal_rustdoc(workspace, "url", "Widget")?;
    write_minimal_rustdoc(workspace, "elicit_url", "Widget")?;

    for crate_name in ["url", "elicit_url"] {
        let source = workspace
            .join("target/doc")
            .join(format!("{crate_name}.json"));
        let crate_root = workspace.join("crates").join(crate_name);
        let local_doc = crate_root.join("doc");
        fs::create_dir_all(&local_doc)
            .into_diagnostic()
            .wrap_err("local doc dir")?;
        fs::copy(&source, local_doc.join(format!("{crate_name}.json")))
            .into_diagnostic()
            .wrap_err("copy local doc")?;
        fs::create_dir_all(store_root.join("cache/rustdoc"))
            .into_diagnostic()
            .wrap_err("store rustdoc dir")?;
        fs::copy(
            &source,
            store_root
                .join("cache/rustdoc")
                .join(format!("{crate_name}.json")),
        )
        .into_diagnostic()
        .wrap_err("copy store rustdoc")?;
    }
    Ok(())
}

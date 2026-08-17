//! Seed synthetic rustdoc for the minimal-workspace impl coverage fixture.

use miette::{IntoDiagnostic, WrapErr};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rustdoc_types::{
    Crate, Generics, Id, Item, ItemEnum, ItemKind, ItemSummary, Module, Struct, StructKind, Target,
    Visibility,
};

use super::CsvTable;

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

pub fn write_minimal_rustdoc_file(
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

/// Seed the same rustdoc inputs used by elicit_doc's pipeline fixture.
pub fn seed_minimal_impl_fixture(workspace: &Path, store_root: &Path) -> miette::Result<()> {
    write_minimal_rustdoc(workspace, "elicitation", "Handle")?;
    write_minimal_rustdoc(workspace, "url", "Widget")?;

    for crate_name in ["elicitation", "url"] {
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

pub fn run_cordial_impl_coverage(
    workspace: &Path,
    store_root: &Path,
    crate_name: Option<&str>,
) -> miette::Result<()> {
    use cordial::{IMPL_COVERAGE_ETIQUETTE, NamedRunFilter, Session, SessionBuilder};

    seed_minimal_impl_fixture(workspace, store_root)?;

    let session = SessionBuilder::new(workspace)
        .with_store_root(store_root)
        .register(&IMPL_COVERAGE_ETIQUETTE)
        .build();

    let filter = match crate_name {
        Some(name) => NamedRunFilter::etiquettes(&["impl-coverage"]).with_crate(name.to_string()),
        None => NamedRunFilter::etiquettes(&["impl-coverage"]),
    };
    session
        .run(&filter)
        .into_diagnostic()
        .wrap_err("cordial impl coverage run")?;
    Ok(())
}

pub const IMPL_GAPS_KEY_COLUMNS: &[&str] = &["type_path", "gap_kind"];

pub fn impl_gaps_open(row: &HashMap<String, String>) -> bool {
    row.get("gap_kind").is_some_and(|kind| !kind.is_empty())
}

/// Map elicit_doc `gaps-impl.csv` columns to cordial's shape for comparison.
pub fn normalize_elicit_impl_gaps(table: &CsvTable) -> CsvTable {
    CsvTable {
        rows: table
            .rows
            .iter()
            .map(|row| {
                let mut out = HashMap::new();
                if let Some(crate_name) = row.get("source_crate") {
                    out.insert("crate".to_string(), crate_name.clone());
                }
                for key in [
                    "type_path",
                    "gap_kind",
                    "missing_our_traits",
                    "missing_external_traits",
                ] {
                    if let Some(value) = row.get(key) {
                        out.insert(key.to_string(), value.clone());
                    }
                }
                out
            })
            .collect(),
    }
}

/// Keep only impl-gap rows for one source crate (elicit `source_crate` / cordial `crate`).
pub fn filter_impl_gaps_by_crate(table: &CsvTable, crate_name: &str) -> CsvTable {
    CsvTable {
        rows: table
            .rows
            .iter()
            .filter(|row| {
                row.get("crate")
                    .or_else(|| row.get("source_crate"))
                    .is_some_and(|name| name == crate_name)
            })
            .cloned()
            .collect(),
    }
}

//! Architecture policy replays from complete IR without syntax or source.
#![cfg(feature = "internal_error_chain")]

#[path = "support/error_sites_default.rs"]
mod default_sites;
#[path = "support/error_sites_projection.rs"]
mod projection;

use cordial::ErrorFieldStrategy;
use cordial::{
    ATTR_SOURCE_ERROR_ARCHITECTURE, CrateIr, CrateTarget, IrView, SourceErrorArchitecture,
};
use miette::{IntoDiagnostic, Result};

const HOOKS: &[&str] = &["error-architecture-inventory"];
const ATTR: &str = "internal_error_record_kind";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn architecture_is_required_and_reassessed_without_source() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'architecture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            "mod empty;\n{}",
            include_str!("fixtures/semantic_backend/error_architecture.rs.txt")
        ),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/empty.rs"), "// no declarations\n").into_diagnostic()?;
    std::fs::write(root.join("src/main.rs"), "mod binary; fn main() {}\n").into_diagnostic()?;
    std::fs::write(
        root.join("src/binary.rs"),
        "#[derive(Error)] struct BinarySource { file: String }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("src/orphan.rs"),
        "#[derive(Error)] struct OrphanSource { file: String }\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = 'borrowed'\n",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("architecture", root);
    let graph = projection::baseline(root, &target)?;
    let inventory = graph.source_error_architecture().into_diagnostic()?;
    assert_eq!(inventory.files().len(), 2);
    assert!(inventory.files().iter().all(|file| matches!(
        file.path().file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "empty.rs")
    )));
    let owned = inventory
        .assess(0, ErrorFieldStrategy::Owned, false)
        .into_diagnostic()?;
    let borrowed = inventory
        .assess(0, ErrorFieldStrategy::Borrowed, false)
        .into_diagnostic()?;
    let fields: Vec<_> = borrowed
        .iter()
        .filter(|finding| finding.rule_id().as_str().ends_with("FIELD-STRATEGY-001"))
        .map(|finding| finding.context().as_str())
        .collect();
    for context in [
        "ParentError::file",
        "ParentKind::Native::_0",
        "ParentKind::Io::_0",
        "NativeSource::message",
        "NativeSource::borrowed_string",
        "NativeSource::bad_generic",
        "NativeSource::source",
        "IoSource::_0",
        "child::ChildSource::file",
    ] {
        assert!(fields.contains(&context), "missing {context}: {fields:?}");
    }
    assert_eq!(
        fields.len(),
        9,
        "Copy fields and the static file alias remain valid"
    );
    let relaxed = inventory
        .assess(100, ErrorFieldStrategy::Owned, false)
        .into_diagnostic()?;
    assert!(
        owned.len() > relaxed.len(),
        "fan-out is reassessed from retained sites"
    );
    let standalone =
        cordial::scan_crate_internal_error_chain(root, "architecture").into_diagnostic()?;
    for finding in &owned {
        assert!(
            standalone.compliance().findings().contains(finding),
            "standalone parity: {finding:?}"
        );
    }
    let encoded = serde_json::to_value(&inventory).into_diagnostic()?;
    let decoded: SourceErrorArchitecture =
        serde_json::from_value(encoded.clone()).into_diagnostic()?;
    assert_eq!(decoded, inventory);
    assert_eq!(
        decoded
            .assess(0, ErrorFieldStrategy::Borrowed, false)
            .into_diagnostic()?,
        borrowed
    );
    for key in ["crate_root", "files", "catalog"] {
        let mut broken = encoded.clone();
        broken
            .as_object_mut()
            .ok_or_else(|| miette::miette!("inventory"))?
            .remove(key);
        let mut corrupt = graph.clone();
        corrupt
            .set_attr(corrupt.root(), ATTR_SOURCE_ERROR_ARCHITECTURE, broken)
            .into_diagnostic()?;
        assert!(
            corrupt.source_error_architecture().is_err(),
            "required {key}"
        );
    }
    for key in [
        "crate_name",
        "structs",
        "enums",
        "constructors",
        "map_err_sites",
        "fanout_wrappers",
        "fanout_calls",
        "error_impls",
        "fields",
        "copy_impls",
    ] {
        let mut broken = encoded.clone();
        broken["catalog"]
            .as_object_mut()
            .ok_or_else(|| miette::miette!("catalog"))?
            .remove(key);
        assert!(
            serde_json::from_value::<SourceErrorArchitecture>(broken).is_err(),
            "required {key}"
        );
    }
    let mut changed = encoded;
    changed["catalog"]["copy_impls"] = serde_json::json!([]);
    let changed: SourceErrorArchitecture = serde_json::from_value(changed).into_diagnostic()?;
    let changed = changed
        .assess(0, ErrorFieldStrategy::Borrowed, false)
        .into_diagnostic()?;
    assert!(
        changed.len() > borrowed.len(),
        "retained Copy contracts must affect policy outcomes"
    );
    assert!(CrateIr::new("empty").source_error_architecture().is_err());
    let rows = projection::materialized_with(&graph, root, HOOKS, ATTR)?;
    assert!(!rows.is_empty());
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert!(!default_sites::materialized(&graph, root)?.is_empty());
    assert_eq!(
        inventory
            .assess(0, ErrorFieldStrategy::Borrowed, false)
            .into_diagnostic()?,
        borrowed
    );
    assert_eq!(
        projection::materialized_with(&graph, root, HOOKS, ATTR)?,
        rows
    );
    Ok(())
}

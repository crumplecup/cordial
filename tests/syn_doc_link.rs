#![cfg(feature = "rustdoc")]

use std::fs;
use std::path::PathBuf;

use cordial::{
    ATTR_IR_ORIGIN, BasicQuery, CordialError, CrateTarget, EtiquetteExplain, EtiquetteHooks,
    IrCacheDigest, IrView, ORIGIN_RUSTDOC, ORIGIN_SOURCE, RunAll, RustdocLoader, Session,
    SessionBuilder, SourceLoader, StaticEtiquette, nightly_available, syn_doc_peer,
};
use miette::{IntoDiagnostic, WrapErr};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static RUSTDOC_LOADER: RustdocLoader = RustdocLoader;

static LOADERS: &[&'static dyn cordial::Loader] = &[&SOURCE_LOADER, &RUSTDOC_LOADER];

static DUAL_INVENTORY_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
    "dual-inventory",
    "Dual inventory",
    EtiquetteHooks::new(LOADERS, &[], &[], &[], None, &[]),
    false,
    EtiquetteExplain::new(
        "Test inventory (not a product lint)",
        "Session fixture used by cordial's own tests.",
        "Loads source (and optionally rustdoc) into IR; emits no findings.",
        "Not registered in the cordial binary.",
        &[],
    ),
);

#[test]
fn syn_doc_link_connects_widget_source_and_rustdoc_nodes() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/build_demo");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(&fixture)
        .with_store_root(store.path())
        .register(&DUAL_INVENTORY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let cache_path = store.path().join("cache").join("build_demo.ir.json");
    let ir = cordial::CrateIr::read_cache(&cache_path)
        .into_diagnostic()
        .wrap_err("read ir cache")?;

    let source = find_item_id(&ir, "Widget", ORIGIN_SOURCE)
        .ok_or_else(|| miette::miette!("source Widget node"))?;
    let rustdoc = find_item_id(&ir, "build_demo::Widget", ORIGIN_RUSTDOC)
        .ok_or_else(|| miette::miette!("rustdoc Widget node"))?;
    assert_ne!(source, rustdoc);

    let source_node = ir
        .node(source)
        .ok_or_else(|| miette::miette!("source node"))?;
    let rustdoc_node = ir
        .node(rustdoc)
        .ok_or_else(|| miette::miette!("rustdoc node"))?;
    assert_eq!(syn_doc_peer(&source_node), Some(rustdoc));
    assert_eq!(syn_doc_peer(&rustdoc_node), Some(source));

    let indexed = ir
        .node_by_path("build_demo::Widget")
        .ok_or_else(|| miette::miette!("path index prefers rustdoc node"))?;
    assert_eq!(indexed, rustdoc);
    Ok(())
}

fn find_item_id(ir: &cordial::CrateIr, path: &str, origin: &str) -> Option<cordial::NodeId> {
    ir.nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .find(|node| {
            node.attr("qualified_path").and_then(|value| value.as_str()) == Some(path)
                && node.attr(ATTR_IR_ORIGIN).and_then(|value| value.as_str()) == Some(origin)
        })
        .map(|node| node.id)
}

#[test]
fn link_key_normalizes_crate_root_items() {
    cordial::init_tracing();
    use cordial::testing::inventory_link_key;

    assert_eq!(
        inventory_link_key("Widget", "build_demo"),
        "build_demo::Widget"
    );
    assert_eq!(
        inventory_link_key("build_demo::Widget", "build_demo"),
        "build_demo::Widget"
    );
}

#[test]
fn missing_rustdoc_json_error_reports_failed_auto_rebuild() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;

    let err = CordialError::missing_rustdoc_json("missing_crate", fixture.path().to_path_buf());
    let message = err.to_string();

    assert!(message.contains("rustdoc JSON not found for crate `missing_crate`"));
    assert!(message.contains("automatic rustdoc cache rebuild did not produce it"));
    assert!(!message.contains("invariant violated"));
    Ok(())
}

#[test]
fn rustdoc_loader_rebuilds_missing_workspace_cache() -> miette::Result<()> {
    cordial::init_tracing();
    if !nightly_available() {
        tracing::warn!("skipping auto-rebuild test: nightly toolchain required for rustdoc JSON");
        return Ok(());
    }

    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("fixture")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        r#"[package]
name = "auto_rustdoc_cache"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub struct Widget;\n")
        .into_diagnostic()
        .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&DUAL_INVENTORY_ETIQUETTE)
        .build();

    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("auto rebuild run")?;

    assert!(
        store
            .path()
            .join("cache/rustdoc/auto_rustdoc_cache.json")
            .is_file()
    );
    assert!(fixture.path().join("doc/auto_rustdoc_cache.json").is_file());
    Ok(())
}

#[test]
fn ir_digest_tolerates_missing_optional_rustdoc_json() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let target = CrateTarget::new("missing_crate", fixture.path());

    let _digest = IrCacheDigest::compute(&target, &[], &Default::default())
        .into_diagnostic()
        .wrap_err("digest")?;
    Ok(())
}

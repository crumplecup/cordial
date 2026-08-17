#![cfg(feature = "rustdoc")]

use std::path::PathBuf;

use cordial::{
    ATTR_IR_ORIGIN, BasicQuery, IrView, ORIGIN_RUSTDOC, ORIGIN_SOURCE, RunAll, RustdocLoader,
    Session, SessionBuilder, SourceLoader, StaticEtiquette, syn_doc_peer,
};
use miette::{IntoDiagnostic, WrapErr};

static SOURCE_LOADER: SourceLoader = SourceLoader;
static RUSTDOC_LOADER: RustdocLoader = RustdocLoader;

static LOADERS: &[&'static dyn cordial::Loader] = &[&SOURCE_LOADER, &RUSTDOC_LOADER];

static DUAL_INVENTORY_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "dual-inventory",
    name: "Dual inventory",
    loaders: LOADERS,
    enrichers: &[],
    probes: &[],
    assessors: &[],
    workspace_assessors: None,
    reporters: &[],
    is_coverage: false,
};

#[test]
fn syn_doc_link_connects_widget_source_and_rustdoc_nodes() -> miette::Result<()> {
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

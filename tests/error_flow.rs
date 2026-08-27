#![cfg(feature = "error_sites")]

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    BasicQuery, CrateIr, ERROR_SITES_ETIQUETTE, EdgeKind, ErrorFlowEnricher, ErrorOriginClass,
    IrView, NodeKind, RunAll, Session, SessionBuilder,
};

const ERROR_SITES: &str = r#"
use cordial::{CordialError, CordialResult};

fn foreign_map_err() -> CordialResult<()> {
    std::fs::read_to_string("x").map_err(CordialError::from)?;
    Ok(())
}

fn propagate_internal(x: CordialResult<()>) -> CordialResult<()> {
    x?;
    Ok(())
}
"#;

#[test]
fn error_flow_enricher_partitions_sites_and_links_origins() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), ERROR_SITES)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&ERROR_SITES_ETIQUETTE)
        .build();

    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    // Not just any `.json` file: `cache/` also holds each crate's
    // `{crate_name}.ir.digests.json` fingerprint cache (`IrCacheDigest`,
    // a different shape entirely, no `root` field) alongside the real
    // `{crate_name}.ir.json` snapshot -- `read_dir`'s order is not
    // guaranteed, so a bare `.json` extension match can non-
    // deterministically pick either one.
    let cache_dir = store.path().join("cache");
    let cache_path = fs::read_dir(&cache_dir)
        .into_diagnostic()
        .wrap_err("cache dir")?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".ir.json"))
        })
        .ok_or_else(|| miette::miette!("ir cache file"))?;
    let ir = CrateIr::read_cache(&cache_path)
        .into_diagnostic()
        .wrap_err("read ir cache")?;

    let sites: Vec<_> = ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| matches!(node.kind(), NodeKind::Expr))
        .filter(|node| node.attr("error_site_kind").is_some())
        .collect();
    assert!(!sites.is_empty(), "expected error-site nodes in IR");

    for site in &sites {
        assert!(
            site.attr("origin_class").is_some(),
            "ErrorFlowEnricher should set origin_class on error-site nodes"
        );
        assert!(
            site.attr("origin_detail").is_some(),
            "ErrorFlowEnricher should set origin_detail on error-site nodes"
        );
    }

    let map_err = sites
        .iter()
        .find(|node| {
            node.attr("site_snippet")
                .and_then(|value| value.as_str())
                .is_some_and(|snippet| snippet.contains("map_err"))
        })
        .ok_or_else(|| miette::miette!("map_err site"))?;
    assert_eq!(
        map_err
            .attr("origin_class")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        ErrorOriginClass::Other.to_string()
    );

    let has_error_flow = sites
        .iter()
        .any(|site| !ir.children(site.id, EdgeKind::ErrorFlow).is_empty());
    assert!(
        has_error_flow,
        "expected ErrorFlow edges to origin stub nodes"
    );

    let origin_count = ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| matches!(node.kind(), NodeKind::Type))
        .filter(|node| {
            node.attr(ErrorFlowEnricher::ATTR_ERROR_FLOW_ORIGIN)
                .and_then(|value| value.as_bool())
                == Some(true)
        })
        .count();
    assert!(
        origin_count > 0,
        "expected deduplicated error-flow origin nodes"
    );
    Ok(())
}

#[test]
fn error_flow_enricher_is_auto_injected_with_error_site_inventory() {
    cordial::init_tracing();
    assert_eq!(ErrorFlowEnricher::ID, "error-flow");
}

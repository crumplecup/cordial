#![cfg(feature = "impl_coverage")]

use cordial::{IMPL_COVERAGE_ETIQUETTE, IrView, NamedRunFilter, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};

mod parity_support;

use parity_support::{run_cordial_impl_coverage, workspace_path};

#[test]
fn path_index_resolves_type_nodes_after_rustdoc_load() -> miette::Result<()> {
    let workspace = workspace_path("minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    run_cordial_impl_coverage(&workspace, store.path(), Some("url"))?;

    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register(&IMPL_COVERAGE_ETIQUETTE)
        .build();
    let filter = NamedRunFilter::etiquettes(&["impl-coverage"]).with_crate("url".to_string());
    session.run(&filter).into_diagnostic().wrap_err("run")?;

    let cache_path = store.path().join("cache").join("url.ir.json");
    let ir = cordial::CrateIr::read_cache(&cache_path)
        .into_diagnostic()
        .wrap_err("read ir cache")?;
    assert!(
        ir.node_by_path("url::Widget").is_some(),
        "path index should resolve url::Widget after PathIndexEnricher"
    );
    Ok(())
}

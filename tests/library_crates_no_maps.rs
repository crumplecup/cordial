//! A library crate (a dependency from a registry, here a vendored one) that a proc-macro crate
//! depends on is digested as an input of what is compiled by its source root's files, and no
//! definition map is built to list them: a cold run's loading would otherwise build the map of every
//! crate reachable from a proc-macro or build-script crate, which is the most it allocates there.

use cordial::{
    CrateTarget, IrView, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout,
    read_run_metrics, write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn a_library_crate_is_digested_by_its_files_and_no_map_is_built_to_list_them() -> Result<()> {
    cordial::init_tracing();
    let base = tempfile::tempdir().into_diagnostic()?;
    let workspace = base.path().join("ws");
    let outside = workspace.join("vendor/outside");
    for (path, text) in [
        (
            workspace.join("Cargo.toml"),
            "[package]\nname='ws'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['macros']\n[dependencies]\nmacros={path='macros'}\n",
        ),
        (workspace.join("src/lib.rs"), "pub fn root() {}\n"),
        (
            workspace.join("macros/Cargo.toml"),
            "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\noutside='0.1.0'\n",
        ),
        (
            workspace.join("macros/src/lib.rs"),
            "use proc_macro::TokenStream;\n#[proc_macro]\npub fn same(input: TokenStream) -> TokenStream { let _ = outside::ready(); input }\n",
        ),
        // crates.io is replaced by the `vendor` directory: the dependency comes from a registry, as
        // far as Cargo and the analysis can tell, and needs no network.
        (
            workspace.join(".cargo/config.toml"),
            "[source.crates-io]\nreplace-with = 'vendored-sources'\n[source.vendored-sources]\ndirectory = 'vendor'\n",
        ),
        (
            outside.join(".cargo-checksum.json"),
            "{\"files\":{},\"package\":null}",
        ),
        (
            outside.join("Cargo.toml"),
            "[package]\nname='outside'\nversion='0.1.0'\nedition='2024'\n",
        ),
        (
            outside.join("src/lib.rs"),
            "pub fn ready() -> bool { true }\n",
        ),
        // A file of the crate that no module declares: the module tree does not reach it.
        (outside.join("src/unlisted.rs"), "pub fn unlisted() {}\n"),
    ] {
        std::fs::create_dir_all(path.parent().ok_or_else(|| miette::miette!("no parent"))?)
            .into_diagnostic()?;
        std::fs::write(path, text).into_diagnostic()?;
    }

    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let mut session =
        SourceDatabase::load_saved(&workspace, &SemanticCargoOptions::default(), None)
            .into_diagnostic()?;
    let graph = session
        .project(
            &CrateTarget::new("ws", &workspace),
            &[workspace.join("src/lib.rs")],
        )
        .into_diagnostic()?;
    let snapshot = graph
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing context"))?;

    let inputs: Vec<std::path::PathBuf> = snapshot
        .workspace()
        .compiled_inputs()
        .iter()
        .map(|input| input.path().clone())
        .collect();
    let canonical =
        |path: &std::path::Path| path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let has = |wanted: std::path::PathBuf| {
        inputs
            .iter()
            .any(|path| canonical(path) == canonical(&wanted))
    };
    assert!(
        has(outside.join("src/lib.rs")),
        "its modules' file: {inputs:?}"
    );
    assert!(
        has(outside.join("src/unlisted.rs")),
        "and a file no module declares, which the source root has: {inputs:?}"
    );

    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "ws".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    let lists: u64 = read_run_metrics(&layout)
        .values()
        .iter()
        .filter(|(key, _)| key.starts_with("steps.") && key.ends_with("> crate_files"))
        .map(|(_, count)| *count)
        .sum();
    // The two crates of the workspace are listed through their module trees (one for what is
    // compiled, one for the projection). The library crate would have been a third.
    assert_eq!(
        lists, 2,
        "no definition map was built to list the library crate's files"
    );
    Ok(())
}

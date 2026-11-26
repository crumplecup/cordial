//! How much of a context's copy of the workspace could be the ordinary build's own files.
//!
//! A context copies every local file under new ids. A file with no gate the context flips, and
//! nothing that reads the cfg some other way, would be the same file in the context; the files above
//! a changed one (the module tree is found through them) are copies too. These tests say, for a
//! crate with one of each, what the context would copy, so that the aliasing can be built against it.

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, MetricsLayer, NodeId, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase, StoreLayout, read_run_metrics, write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

/// The files of the crate: a path and its text.
const FILES: [(&str, &str); 7] = [
    (
        "src/lib.rs",
        "mod gated;\nmod plain;\nmod plain_too;\nmod other;\nmod nested;\npub fn root() {}\n",
    ),
    (
        "src/gated.rs",
        "pub fn always() {}\n#[cfg(kani)]\npub fn proof() { crate::plain::a(); }\n",
    ),
    ("src/plain.rs", "pub fn a() {}\npub fn b() { a(); }\n"),
    ("src/plain_too.rs", "pub fn c() {}\n"),
    ("src/other.rs", "pub fn reads() -> bool { cfg!(kani) }\n"),
    ("src/nested/mod.rs", "mod deep;\npub fn middle() {}\n"),
    (
        "src/nested/deep.rs",
        "#[cfg(not(kani))]\npub fn deep() {}\n#[cfg(kani)]\npub fn deep_proof() { crate::plain_too::c(); }\n",
    ),
];

fn kani() -> SemanticContextKey {
    SemanticContextKey::enabling(["kani"])
}

/// The crate projected with the kani context, and what the run said.
fn project() -> Result<(CrateIr, cordial::RunMetrics)> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='reuse'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let mut sources = Vec::new();
    for (path, text) in FILES {
        let file = root.path().join(path);
        std::fs::create_dir_all(file.parent().ok_or_else(|| miette::miette!("no parent"))?)
            .into_diagnostic()?;
        std::fs::write(&file, text).into_diagnostic()?;
        sources.push(file);
    }
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("reuse", root.path()), &sources)
        .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "reuse".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    Ok((graph, read_run_metrics(&layout)))
}

fn function(graph: &CrateIr, name: &str) -> Result<NodeId> {
    let found: Vec<NodeId> = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id())
        .collect();
    match found.as_slice() {
        [id] => Ok(*id),
        other => Err(miette::miette!("{name}: {other:?}")),
    }
}

/// What the context finds through the files it takes as they are: its gated code, in copied files,
/// calls functions of the files it did not copy, and those functions are active in it.
#[test]
fn a_context_reaches_the_files_it_takes_as_they_are() -> Result<()> {
    let (graph, _) = project()?;
    let (proof, deep_proof) = (function(&graph, "proof")?, function(&graph, "deep_proof")?);
    let (a, c) = (function(&graph, "a")?, function(&graph, "c")?);

    // The gated function of a copied file calls a function of a file that was not copied.
    assert_eq!(graph.context_children(proof, &kani()), vec![a]);
    // And so does one under a module whose file is a copy only for the module tree's sake.
    assert_eq!(graph.context_children(deep_proof, &kani()), vec![c]);
    assert_eq!(graph.context_parents(a, &kani()), vec![proof]);

    // The functions of the files it did not copy are active in it, as they are in the ordinary build.
    let mut contexts = vec![SemanticContextKey::ordinary(), kani()];
    contexts.sort();
    for function in [a, c, function(&graph, "b")?] {
        let mut active = graph
            .active_contexts(function)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("no semantic projection"))?;
        active.sort();
        assert_eq!(active, contexts);
    }
    // And gated code is active only there.
    assert_eq!(
        graph.active_contexts(proof).into_diagnostic()?,
        Some(vec![kani()])
    );
    Ok(())
}

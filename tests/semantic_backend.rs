//! Integration, semantic identity, and revision checks for the shared source database.

mod support {
    pub mod semantic_metrics;
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cordial::SourceDatabase;
use cordial::{
    CrateIr, CrateTarget, IrView, QueryBuilder, SemanticCallTarget, SemanticFunction,
    SemanticReturnContract, SemanticSnapshot, SemanticType,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::prelude::*;

const LEAF: &str = include_str!("fixtures/semantic_backend/leaf.rs.txt");

#[tracing::instrument(level = "debug", err(level = "warn"))]
fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"),
        "[package]\nname = \"semantic_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    for (file, text) in [
        (
            "lib.rs",
            "pub mod leaf; pub mod middle; pub mod unrelated; pub mod shadow; pub mod unresolved;\npub fn root() { middle::middle(); }\n",
        ),
        ("leaf.rs", LEAF),
        (
            "middle.rs",
            include_str!("fixtures/semantic_backend/middle.rs.txt"),
        ),
        ("unrelated.rs", "pub fn unrelated() -> u32 { 7 }\n"),
        (
            "shadow.rs",
            "pub enum Result<T, E> { Value(T), Failure(E) }\npub fn fake() -> Result<u32, crate::leaf::ReadFailure> { Result::Value(4) }\npub struct SizedFailure<const N: usize>;\npub fn const_error() -> core::result::Result<u32, SizedFailure<4>> { Ok(4) }\npub struct BorrowedFailure<'a>(&'a str);\npub fn lifetime_error() -> core::result::Result<u32, BorrowedFailure<'static>> { Ok(4) }\n",
        ),
        ("unresolved.rs", "pub fn missing() { not_a_function(); }\n"),
    ] {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    Ok(root)
}

#[tracing::instrument(level = "debug", skip(root))]
fn paths(root: &Path) -> Vec<PathBuf> {
    [
        "lib.rs",
        "leaf.rs",
        "middle.rs",
        "unrelated.rs",
        "shadow.rs",
        "unresolved.rs",
    ]
    .map(|file| root.join("src").join(file))
    .into()
}

#[tracing::instrument(level = "debug", skip(ir), err(level = "warn"))]
fn function(ir: &dyn IrView, label: &str) -> Result<SemanticFunction> {
    let query = QueryBuilder::new()
        .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
        .build();
    for node in ir.nodes_matching(&query) {
        let facts = ir
            .semantic_function(node.id())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing semantic function payload"))?;
        if facts.label() == label {
            return Ok(facts);
        }
    }
    Err(miette::miette!("function {label} missing from graph"))
}

#[tracing::instrument(level = "debug", skip(root))]
fn semantic_target(root: &Path) -> CrateTarget {
    CrateTarget::new("semantic_specimen", root)
}

/// The `Result` alias resolves to its success and error types, every resolved call carries the
/// callee's contract, unresolved and unsupported shapes are marked incomplete, and the calls are
/// linked as graph edges.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_results_resolve_across_files(graph: &CrateIr) -> Result<()> {
    let leaf = function(graph, "fallible")?;
    let method = function(graph, "read")?;
    let middle = function(graph, "middle")?;
    let SemanticReturnContract::Result {
        definition,
        success,
        error,
    } = leaf.output()
    else {
        return Err(miette::miette!(
            "Result alias was not resolved: {:?}",
            leaf.output()
        ));
    };
    assert_eq!(success, &SemanticType::Scalar("u32".into()));
    assert!(matches!(error, SemanticType::Adt { label, .. } if label == "ReadFailure"));
    for target in [leaf.definition(), method.definition()] {
        let call = middle
            .calls()
            .iter()
            .find(|call| call.target() == &SemanticCallTarget::Function(*target));
        let Some(call) = call else {
            return Err(miette::miette!("missing resolved source call {target:?}"));
        };
        assert!(
            matches!(call.output(), SemanticReturnContract::Result { definition: actual, error: actual_error, .. }
            if actual == definition && actual_error == error)
        );
    }
    assert!(matches!(
        function(graph, "fake")?.output(),
        SemanticReturnContract::Other(_)
    ));
    for label in ["const_error", "lifetime_error"] {
        let facts = function(graph, label)?;
        assert!(matches!(
            facts.output(),
            SemanticReturnContract::Result {
                error: SemanticType::Unsupported,
                ..
            }
        ));
        assert!(*facts.incomplete());
    }
    let missing = function(graph, "missing")?;
    assert_eq!(missing.calls().len(), 1);
    assert_eq!(missing.calls()[0].target(), &SemanticCallTarget::Unresolved);
    assert!(*missing.incomplete());
    assert_eq!(
        function(graph, "root")?.reachable_result_errors(),
        &vec![error.clone()]
    );
    let mut middle_node = None;
    for node in graph.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
            .build(),
    ) {
        if graph
            .semantic_function(node.id())
            .into_diagnostic()?
            .as_ref()
            == Some(&middle)
        {
            middle_node = Some(node.id());
            break;
        }
    }
    let middle_node = middle_node.ok_or_else(|| miette::miette!("middle node missing"))?;
    let linked: Vec<_> = graph
        .neighbors(
            middle_node,
            cordial::EdgeKind::Calls,
            petgraph::Direction::Outgoing,
        )
        .into_iter()
        .map(|(_, id)| graph.semantic_function(id))
        .collect::<cordial::CordialResult<Vec<_>>>()
        .into_diagnostic()?
        .into_iter()
        .flatten()
        .map(|facts| *facts.definition())
        .collect();
    assert!(linked.contains(leaf.definition()));
    assert!(linked.contains(method.definition()));
    Ok(())
}

/// The first projection is revision 1 and records all six sources; the snapshot is returned so the
/// later phases can compare against it.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn initial_snapshot_of(graph: &CrateIr) -> Result<SemanticSnapshot> {
    let snapshot = graph
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing semantic snapshot"))?;
    assert_eq!(*snapshot.revision(), 1);
    assert_eq!(snapshot.sources().len(), 6);
    Ok(snapshot)
}

/// Projecting again with nothing changed returns the same facts and runs no Cordial query.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_warm_projection_runs_no_queries(
    session: &mut SourceDatabase,
    root: &Path,
    first: &CrateIr,
    metrics: &support::semantic_metrics::QueryMetrics,
    before: &BTreeMap<String, usize>,
) -> Result<()> {
    let warm = session
        .project(&semantic_target(root), &paths(root))
        .into_diagnostic()?;
    assert_eq!(function(first, "root")?, function(&warm, "root")?);
    assert_eq!(
        &metrics.snapshot(),
        before,
        "a warm projection must execute no Cordial queries"
    );
    Ok(())
}

/// Changing one file advances the revision once, changes one source, reaches every function that
/// can return the new error, and recomputes nothing about the unrelated module. The edit is
/// virtual: the file on disk keeps its text.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_edit_invalidates_only_dependents(
    session: &mut SourceDatabase,
    root: &Path,
    first: &CrateIr,
    initial: &SemanticSnapshot,
    metrics: &support::semantic_metrics::QueryMetrics,
    before: &BTreeMap<String, usize>,
) -> Result<()> {
    let leaf_path = root.join("src/leaf.rs");
    session
        .change_file(
            &leaf_path,
            LEAF.replace("Result<T, ReadFailure>", "Result<T, OtherFailure>"),
        )
        .into_diagnostic()?;
    assert!(
        session
            .change_file(&root.join("src/absent.rs"), "fn absent() {}".into())
            .is_err()
    );
    let revised = session
        .project(&semantic_target(root), &paths(root))
        .into_diagnostic()?;
    let revised_snapshot = revised
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing revised snapshot"))?;
    assert_eq!(
        *revised_snapshot.revision(),
        2,
        "failed changes must not advance the revision"
    );
    assert_eq!(
        initial
            .sources()
            .iter()
            .zip(revised_snapshot.sources())
            .filter(|(old, new)| old != new)
            .count(),
        1
    );
    for label in ["fallible", "read", "middle", "root"] {
        let old = function(first, label)?;
        let new = function(&revised, label)?;
        assert_ne!(
            old.reachable_result_errors(),
            new.reachable_result_errors(),
            "{label}"
        );
        assert!(
            new.reachable_result_errors()
                .iter()
                .any(|ty| matches!(ty, SemanticType::Adt { label, .. } if label == "OtherFailure"))
        );
    }
    let after = metrics.snapshot();
    for query in ["direct:unrelated", "reachable:unrelated"] {
        assert_eq!(
            after.get(query),
            before.get(query),
            "unrelated query recomputed: {query}"
        );
    }
    assert!(after.get("reachable:root") > before.get("reachable:root"));
    assert_eq!(
        std::fs::read_to_string(leaf_path).into_diagnostic()?,
        LEAF,
        "candidate must be virtual"
    );
    Ok(())
}

/// Rejecting the candidate means restoring the inputs as a new revision, which yields the original
/// facts and sources again.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_restore_returns_to_original_facts(
    session: &mut SourceDatabase,
    root: &Path,
    first: &CrateIr,
    initial: &SemanticSnapshot,
) -> Result<()> {
    session
        .change_file(&root.join("src/leaf.rs"), LEAF.to_owned())
        .into_diagnostic()?;
    let restored = session
        .project(&semantic_target(root), &paths(root))
        .into_diagnostic()?;
    assert_eq!(function(first, "root")?, function(&restored, "root")?);
    let restored_snapshot = restored
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing restored snapshot"))?;
    assert_eq!(*restored_snapshot.revision(), 3);
    assert_eq!(initial.sources(), restored_snapshot.sources());
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn shared_database_resolves_and_invalidates_cross_file_evidence() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry()
        .with(metrics.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = session
        .project(&semantic_target(root.path()), &paths(root.path()))
        .into_diagnostic()?;
    assert_results_resolve_across_files(&graph)?;
    let initial = initial_snapshot_of(&graph)?;
    let first = session
        .project(&semantic_target(root.path()), &paths(root.path()))
        .into_diagnostic()?;
    let before = metrics.snapshot();
    assert!(
        before.contains_key("direct:unrelated"),
        "query counts: {before:?}"
    );
    assert_warm_projection_runs_no_queries(&mut session, root.path(), &first, &metrics, &before)?;
    assert_edit_invalidates_only_dependents(
        &mut session,
        root.path(),
        &first,
        &initial,
        &metrics,
        &before,
    )?;
    assert_restore_returns_to_original_facts(&mut session, root.path(), &first, &initial)
}

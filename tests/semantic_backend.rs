//! Integration, semantic identity, and revision checks for the shared source database.

mod support {
    pub mod semantic_metrics;
}

use std::path::{Path, PathBuf};

use cordial::SourceDatabase;
use cordial::{
    IrView, QueryBuilder, SemanticCallTarget, SemanticFunction, SemanticReturnContract,
    SemanticType,
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

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn shared_database_resolves_and_invalidates_cross_file_evidence() -> Result<()> {
    cordial::init_tracing();
    let metrics = support::semantic_metrics::QueryMetrics::default();
    let subscriber = tracing_subscriber::registry().with(metrics.clone());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let root = specimen()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = session
        .project(
            &cordial::CrateTarget::new("semantic_specimen", root.path()),
            &paths(root.path()),
        )
        .into_diagnostic()?;
    let leaf = function(&graph, "fallible")?;
    let method = function(&graph, "read")?;
    let middle = function(&graph, "middle")?;
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
        function(&graph, "fake")?.output(),
        SemanticReturnContract::Other(_)
    ));
    for label in ["const_error", "lifetime_error"] {
        let facts = function(&graph, label)?;
        assert!(matches!(
            facts.output(),
            SemanticReturnContract::Result {
                error: SemanticType::Unsupported,
                ..
            }
        ));
        assert!(*facts.incomplete());
    }
    let missing = function(&graph, "missing")?;
    assert_eq!(missing.calls().len(), 1);
    assert_eq!(missing.calls()[0].target(), &SemanticCallTarget::Unresolved);
    assert!(*missing.incomplete());
    assert_eq!(
        function(&graph, "root")?.reachable_result_errors(),
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
    let initial_snapshot = graph
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing semantic snapshot"))?;
    assert_eq!(*initial_snapshot.revision(), 1);
    assert_eq!(initial_snapshot.sources().len(), 6);
    let paths = paths(root.path());
    let first = session
        .project(
            &cordial::CrateTarget::new("semantic_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    let before = metrics.snapshot();
    assert!(
        before.contains_key("direct:unrelated"),
        "query counts: {before:?}"
    );
    let warm = session
        .project(
            &cordial::CrateTarget::new("semantic_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    assert_eq!(function(&first, "root")?, function(&warm, "root")?);
    assert_eq!(
        metrics.snapshot(),
        before,
        "a warm projection must execute no Cordial queries"
    );
    let leaf_path = root.path().join("src/leaf.rs");
    session
        .change_file(
            &leaf_path,
            LEAF.replace("Result<T, ReadFailure>", "Result<T, OtherFailure>"),
        )
        .into_diagnostic()?;
    assert!(
        session
            .change_file(&root.path().join("src/absent.rs"), "fn absent() {}".into())
            .is_err()
    );
    let revised = session
        .project(
            &cordial::CrateTarget::new("semantic_specimen", root.path()),
            &paths,
        )
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
        initial_snapshot
            .sources()
            .iter()
            .zip(revised_snapshot.sources())
            .filter(|(old, new)| old != new)
            .count(),
        1
    );
    for label in ["fallible", "read", "middle", "root"] {
        let old = function(&first, label)?;
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
    // Reject the candidate by restoring inputs as a new revision.
    session
        .change_file(&root.path().join("src/leaf.rs"), LEAF.to_owned())
        .into_diagnostic()?;
    let restored = session
        .project(
            &cordial::CrateTarget::new("semantic_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    assert_eq!(function(&first, "root")?, function(&restored, "root")?);
    let restored_snapshot = restored
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing restored snapshot"))?;
    assert_eq!(*restored_snapshot.revision(), 3);
    assert_eq!(initial_snapshot.sources(), restored_snapshot.sources());
    Ok(())
}

//! Verus dialect facts survive projection, virtual revisions, and IR persistence.
use super::{semantic_metrics, shared};
use cordial::{
    ATTR_SOURCE_VERUS, CrateIr, CrateTarget, IrView, LoadContext, Loader, SessionBuilder,
    SourceDatabase, SourceLoadView, SourceLoader, SourceVerus,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use tracing_subscriber::prelude::*;

pub const PACKAGE: &str = "verus_parity";

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    let source = [
        include_str!("../fixtures/verus_ir/signature_facts_source.rs"),
        include_str!("../fixtures/verus_ir/cfg_test_and_compile_error_source.rs"),
        include_str!("../fixtures/verus_ir/documented_data_carrying_enum_source.rs"),
        include_str!("../fixtures/verus_ir/ghost_exec_unreachable_source.rs"),
        include_str!("../fixtures/verus_ir/assume_and_axiom_source.rs"),
        include_str!("../fixtures/verus_ir/view_operator_source.rs"),
        // Best-effort dialect coverage remains explicit: qualified macros are
        // outside the existing inventory, malformed payloads contribute no rows.
        include_str!("../fixtures/verus_ir/backend_edges_source.rs"),
    ]
    .join("\n");
    let path = root.join("src/lib.rs");
    std::fs::write(&path, &source).into_diagnostic()?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn check(root: &Path, database: &mut SourceDatabase) -> Result<()> {
    let metrics = semantic_metrics::QueryMetrics::default();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(metrics.clone())
            .with(cordial::tracing_fmt_layer()),
    );
    let source = [
        include_str!("../fixtures/verus_ir/signature_facts_source.rs"),
        include_str!("../fixtures/verus_ir/cfg_test_and_compile_error_source.rs"),
        include_str!("../fixtures/verus_ir/documented_data_carrying_enum_source.rs"),
        include_str!("../fixtures/verus_ir/ghost_exec_unreachable_source.rs"),
        include_str!("../fixtures/verus_ir/assume_and_axiom_source.rs"),
        include_str!("../fixtures/verus_ir/view_operator_source.rs"),
        // Best-effort dialect coverage remains explicit: qualified macros are
        // outside the existing inventory, malformed payloads contribute no rows.
        include_str!("../fixtures/verus_ir/backend_edges_source.rs"),
    ]
    .join("\n");
    let path = root.join("src/lib.rs");
    let target = CrateTarget::new(PACKAGE, root);
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let loader = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut baseline = CrateIr::new("verus_parity");
    loader.populate_ir(&mut baseline).into_diagnostic()?;
    let expected = baseline.source_verus().into_diagnostic()?;
    // The baseline loader parses through its own database now, so count only
    // what the database under test adds.
    let baseline_payloads = metrics
        .snapshot()
        .get("verus_dialect:payload")
        .copied()
        .unwrap_or_default();
    let native = database.project(&target, &[]).into_diagnostic()?;
    let actual = native.source_verus().into_diagnostic()?;
    let counts = metrics.snapshot();
    assert_eq!(
        counts
            .get("verus_dialect:payload")
            .copied()
            .unwrap_or_default()
            - baseline_payloads,
        9,
        "seven fixture blocks plus two bare edge blocks parse once, including the malformed body"
    );
    database.project(&target, &[]).into_diagnostic()?;
    assert_eq!(
        metrics.snapshot(),
        counts,
        "unchanged projections reuse dialect facts"
    );
    #[cfg(feature = "visibility")]
    assert_eq!(
        baseline
            .source_policy_inventory()
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing visibility inventory"))?
            .visibility(),
        native
            .source_policy_inventory()
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing visibility inventory"))?
            .visibility(),
        "visibility shares the same dialect declarations"
    );
    assert_eq!(
        serde_json::to_value(&actual).into_diagnostic()?,
        serde_json::to_value(&expected).into_diagnostic()?,
        "every fact and source snapshot must match"
    );
    #[cfg(feature = "proof_patterns")]
    let proof_rows = materialized_proof(&native)?;
    #[cfg(feature = "proof_patterns")]
    assert!(!proof_rows.is_empty());
    let facts = actual.facts();
    assert!(facts.functions().len() > 8);
    assert!(!facts.enums().is_empty());
    assert!(facts.functions().iter().any(|function| function.cfg_test()));
    assert!(
        facts
            .panic_sites()
            .any(|(_, site)| site.proven_unreachable_by_ghost_sibling())
    );
    let encoded = serde_json::to_value(&actual).into_diagnostic()?;
    assert_eq!(
        serde_json::from_value::<SourceVerus>(encoded.clone()).into_diagnostic()?,
        actual
    );
    let mut malformed = encoded;
    malformed["files"][0]["facts"]
        .as_object_mut()
        .ok_or_else(|| miette::miette!("facts object"))?
        .remove("functions");
    assert!(serde_json::from_value::<SourceVerus>(malformed).is_err());
    assert!(CrateIr::new("missing").source_verus().is_err());
    let mut corrupted = native.clone();
    corrupted
        .set_attr(
            corrupted.root(),
            ATTR_SOURCE_VERUS,
            serde_json::json!({"files": []}),
        )
        .into_diagnostic()?;
    assert!(corrupted.source_verus().is_err());
    database
        .change_file(
            &path,
            source
                .replace("assume(", "assert(")
                .replace("panic!", "unreachable!"),
        )
        .into_diagnostic()?;
    let changed = database
        .project(&target, &[])
        .into_diagnostic()?
        .source_verus()
        .into_diagnostic()?;
    assert_ne!(actual.files()[0].snapshot(), changed.files()[0].snapshot());
    assert_ne!(
        actual.facts(),
        changed.facts(),
        "same declarations must not retain old body facts"
    );
    assert_eq!(std::fs::read_to_string(&path).into_diagnostic()?, source);
    database.change_file(&path, source).into_diagnostic()?;
    assert_eq!(
        database
            .project(&target, &[])
            .into_diagnostic()?
            .source_verus()
            .into_diagnostic()?,
        actual
    );
    database
        .change_file(&path, "fn invalid( {".into())
        .into_diagnostic()?;
    assert!(
        database.project(&target, &[]).is_err(),
        "invalid outer Rust cannot produce complete dialect evidence"
    );
    shared::resync(database, root)?;
    drop(loaded);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    assert_eq!(
        native.source_verus().into_diagnostic()?.facts(),
        facts,
        "consumers need only persisted IR"
    );
    #[cfg(feature = "proof_patterns")]
    assert_eq!(
        materialized_proof(&native)?,
        proof_rows,
        "proof hooks consume IR after deleting source"
    );
    Ok(())
}

#[cfg(feature = "proof_patterns")]
#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn materialized_proof(graph: &CrateIr) -> Result<Vec<serde_json::Value>> {
    use cordial::{EnrichView, Etiquette, PROOF_PATTERNS_ETIQUETTE, QueryBuilder};
    let empty = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(empty.path()).build();
    let target = CrateTarget::new("empty", empty.path());
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let hook = PROOF_PATTERNS_ETIQUETTE
        .enrichers()
        .iter()
        .find(|hook| hook.id() == "proof-pattern-inventory")
        .ok_or_else(|| miette::miette!("proof hook"))?;
    let mut graph = graph.clone();
    hook.enrich(EnrichView::new(&mut graph, loaded.as_ref(), &session))
        .into_diagnostic()?;
    let mut rows = Vec::new();
    for node in graph.nodes_matching(&QueryBuilder::new().has_attr("proof_pattern_kind").build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs": attrs, "span": node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}

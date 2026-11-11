//! The same files are analysed in each requested cfg context inside one database.

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, EdgeKind, IrView, NodeId, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const LIBRARY: &str = "\
pub fn helper() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(kani)]
pub fn harness() { let _ = helper(); }

#[cfg(creusot)]
pub fn creusot_harness() { let _ = helper(); }

#[cfg(not(kani))]
pub fn ordinary_only() {}

#[cfg(kani)]
pub fn exclusive() {}

#[cfg(creusot)]
pub fn exclusive() {}
";

fn kani() -> SemanticContextKey {
    SemanticContextKey::enabling(["kani"])
}

fn creusot() -> SemanticContextKey {
    SemanticContextKey::enabling(["creusot"])
}

const NESTED: &str = "\
pub fn helper() {}

#[cfg(feature = \"a\")]
pub mod m {
    #[cfg(feature = \"b\")]
    pub fn deep() { crate::helper(); }
    pub fn shallow() {}
}

#[cfg(not(feature = \"a\"))]
pub fn without_a() {}
";

fn fixture() -> Result<tempfile::TempDir> {
    fixture_with(LIBRARY)
}

fn fixture_with(library: &str) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

/// Only the named contexts, with no planning.
fn project(contexts: Vec<SemanticContextKey>, root: &Path) -> Result<CrateIr> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(contexts)
        .with_cfg_planning(CfgPlanning::Explicit);
    project_with(&options, root)
}

fn project_with(options: &SemanticCargoOptions, root: &Path) -> Result<CrateIr> {
    let mut session = SourceDatabase::load_with(root, options).into_diagnostic()?;
    session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()
}

/// Every function node with this name, in source order.
fn functions(graph: &CrateIr, name: &str) -> Vec<NodeId> {
    let mut found: Vec<_> = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id())
        .collect();
    found.sort_by_key(|id| id.value());
    found
}

fn only(graph: &CrateIr, name: &str) -> Result<NodeId> {
    match functions(graph, name).as_slice() {
        [id] => Ok(*id),
        other => Err(miette::miette!("{name}: {other:?}")),
    }
}

fn active(graph: &CrateIr, id: NodeId) -> Result<Vec<SemanticContextKey>> {
    let mut contexts = graph
        .active_contexts(id)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no semantic projection"))?;
    contexts.sort();
    Ok(contexts)
}

fn plan(graph: &CrateIr) -> Result<cordial::SemanticContextPlan> {
    graph
        .semantic_context_plan()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing context plan"))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn ordinary_load_leaves_gated_code_explicitly_inactive() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let graph = project(Vec::new(), root.path())?;
    let plan = plan(&graph)?;
    assert!(plan.contexts().is_empty() && *plan.complete(), "{plan:?}");
    let ordinary = vec![SemanticContextKey::ordinary()];
    assert_eq!(active(&graph, only(&graph, "helper")?)?, ordinary);
    assert_eq!(active(&graph, only(&graph, "ordinary_only")?)?, ordinary);
    // Inactive in every loaded context: explicit, not an empty fact set.
    assert!(active(&graph, only(&graph, "harness")?)?.is_empty());
    assert!(
        graph
            .semantic_function(only(&graph, "harness")?)
            .into_diagnostic()?
            .is_none()
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_resolves_gated_code_and_its_calls_without_touching_ordinary_edges() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let graph = project(vec![kani()], root.path())?;
    let plan = plan(&graph)?;
    assert_eq!(plan.contexts(), &[kani()]);
    assert!(*plan.complete());
    let (helper, harness) = (only(&graph, "helper")?, only(&graph, "harness")?);

    assert_eq!(active(&graph, harness)?, vec![kani()]);
    assert!(
        graph
            .semantic_function(harness)
            .into_diagnostic()?
            .is_none(),
        "gated code has no ordinary facts"
    );
    let entries = graph
        .semantic_context_functions(harness)
        .into_diagnostic()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].context(), &kani());
    assert_eq!(entries[0].function().calls().len(), 1);
    assert!(
        matches!(
            entries[0].function().calls()[0].target(),
            cordial::SemanticCallTarget::Function(_)
        ),
        "{:?}",
        entries[0].function().calls()[0]
    );

    // The gated caller reaches `helper` only in the kani context.
    assert_eq!(graph.context_children(harness, &kani()), vec![helper]);
    assert_eq!(graph.context_parents(helper, &kani()), vec![harness]);
    assert!(graph.children(harness, EdgeKind::Calls).is_empty());
    assert!(graph.parents(helper, EdgeKind::Calls).is_empty());
    assert!(graph.context_parents(helper, &creusot()).is_empty());

    // Code the context turns off is no longer active there.
    assert_eq!(
        active(&graph, only(&graph, "ordinary_only")?)?,
        vec![SemanticContextKey::ordinary()]
    );
    let mut helper_contexts = vec![SemanticContextKey::ordinary(), kani()];
    helper_contexts.sort();
    assert_eq!(active(&graph, helper)?, helper_contexts);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn exclusive_verifier_cfgs_get_separate_contexts() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let graph = project(vec![kani(), creusot()], root.path())?;
    let helper = only(&graph, "helper")?;

    // `exclusive` is defined twice, once per verifier; neither context sees both.
    let exclusive = functions(&graph, "exclusive");
    assert_eq!(exclusive.len(), 2, "{exclusive:?}");
    let mut seen = Vec::new();
    for id in exclusive {
        seen.push(active(&graph, id)?);
    }
    seen.sort();
    assert_eq!(seen, vec![vec![creusot()], vec![kani()]]);

    assert_eq!(
        graph.context_children(only(&graph, "harness")?, &kani()),
        vec![helper]
    );
    assert_eq!(
        graph.context_children(only(&graph, "creusot_harness")?, &creusot()),
        vec![helper]
    );
    let mut callers_in_kani = graph.context_parents(helper, &kani());
    callers_in_kani.sort_by_key(|id| id.value());
    assert_eq!(callers_in_kani, vec![only(&graph, "harness")?]);
    assert_eq!(
        graph.context_parents(helper, &creusot()),
        vec![only(&graph, "creusot_harness")?]
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn planning_finds_verifier_contexts_without_being_told() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let graph = project_with(&SemanticCargoOptions::default(), root.path())?;
    let helper = only(&graph, "helper")?;
    assert_eq!(active(&graph, only(&graph, "harness")?)?, vec![kani()]);
    assert_eq!(
        active(&graph, only(&graph, "creusot_harness")?)?,
        vec![creusot()]
    );
    assert_eq!(
        graph.context_parents(helper, &kani()),
        vec![only(&graph, "harness")?]
    );
    // `not(kani)` already holds in the ordinary build; no context is asked for it.
    assert_eq!(
        active(&graph, only(&graph, "ordinary_only")?)?,
        vec![SemanticContextKey::ordinary(), creusot()]
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn planning_composes_nested_gates_and_key_value_atoms() -> Result<()> {
    cordial::init_tracing();
    let root = fixture_with(NESTED)?;
    let graph = project_with(&SemanticCargoOptions::default(), root.path())?;
    let ab = SemanticContextKey::enabling(["feature=\"a\"", "feature=\"b\""]);

    // `deep` needs both features at once; neither alone reaches it. The two
    // requests are compatible, so they share one context, which holds both.
    assert_eq!(plan(&graph)?.contexts(), std::slice::from_ref(&ab));
    assert_eq!(active(&graph, only(&graph, "deep")?)?, vec![ab.clone()]);
    assert_eq!(active(&graph, only(&graph, "shallow")?)?, vec![ab.clone()]);
    assert_eq!(
        active(&graph, only(&graph, "without_a")?)?,
        vec![SemanticContextKey::ordinary()]
    );
    assert_eq!(
        graph.context_children(only(&graph, "deep")?, &ab),
        vec![only(&graph, "helper")?]
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_spent_memory_budget_leaves_an_incomplete_plan_not_a_failure() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let options = SemanticCargoOptions::default().with_context_memory_budget_mib(Some(0));
    let graph = project_with(&options, root.path())?;
    let plan = plan(&graph)?;
    assert!(plan.contexts().is_empty(), "{plan:?}");
    assert!(!plan.complete(), "{plan:?}");
    // Gated code stays explicitly inactive; the ordinary facts are untouched.
    assert!(active(&graph, only(&graph, "harness")?)?.is_empty());
    assert_eq!(
        active(&graph, only(&graph, "helper")?)?,
        vec![SemanticContextKey::ordinary()]
    );
    Ok(())
}

const INDEPENDENT: &str = "\
#[cfg(feature = \"a\")]
pub fn only_a() {}

#[cfg(feature = \"b\")]
pub fn only_b() {}

#[cfg(kani)]
pub fn same() {}

#[cfg(creusot)]
pub fn same() {}
";

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn compatible_requests_share_contexts_and_exclusive_ones_never_do() -> Result<()> {
    cordial::init_tracing();
    let root = fixture_with(INDEPENDENT)?;
    let graph = project_with(&SemanticCargoOptions::default(), root.path())?;
    let analysed = plan(&graph)?.contexts().to_vec();
    let enables = |context: &SemanticContextKey, atom: &str| {
        context.enabled().iter().any(|enabled| enabled == atom)
    };

    // Four requests fit in fewer contexts, and every gate is switched on somewhere.
    assert!(analysed.len() < 4, "{analysed:?}");
    for atom in ["feature=\"a\"", "feature=\"b\"", "kani", "creusot"] {
        assert!(
            analysed.iter().any(|context| enables(context, atom)),
            "{atom}"
        );
    }
    // `kani` and `creusot` each define `same`; no context may hold both.
    assert!(
        analysed
            .iter()
            .all(|context| !(enables(context, "kani") && enables(context, "creusot"))),
        "{analysed:?}"
    );
    // Each gated function is active in exactly the contexts that switch it on.
    for (name, atom) in [("only_a", "feature=\"a\""), ("only_b", "feature=\"b\"")] {
        let contexts = active(&graph, only(&graph, name)?)?;
        assert!(!contexts.is_empty());
        assert!(contexts.iter().all(|context| enables(context, atom)));
    }
    let mut same: Vec<_> = functions(&graph, "same")
        .into_iter()
        .map(|id| active(&graph, id))
        .collect::<Result<_>>()?;
    same.sort_by_key(|contexts| contexts.iter().any(|context| enables(context, "kani")));
    assert!(same[0].iter().all(|context| enables(context, "creusot")));
    assert!(same[1].iter().all(|context| enables(context, "kani")));
    Ok(())
}

//! A declaration that exists only under a cfg context is described by where it is written, not by
//! which context's copy of the file it was read from: the same declaration seen from context 0 of
//! one run and context 1 of another has one description, so a fact saved about it is found again.

use std::path::Path;

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, NodeId, SemanticCallTarget, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = "\
mod far;

pub fn helper() {}

#[cfg(kani)]
pub fn harness() { gated(); }

#[cfg(kani)]
pub fn gated() {}

#[cfg(creusot)]
pub fn other() { helper(); }

#[cfg(kani)]
pub fn reaches() { far::away(); }

#[cfg(kani)]
pub fn exclusive() {}

#[cfg(creusot)]
pub fn exclusive() {}

#[cfg(kani)]
pub fn kani_caller() { exclusive(); }

#[cfg(creusot)]
pub fn creusot_caller() { exclusive(); }
";

/// A second file of the crate, which the projection is not asked about.
const FAR: &str = "\
#[cfg(kani)]
pub fn away() {}
";

fn kani() -> SemanticContextKey {
    SemanticContextKey::enabling(["kani"])
}

fn creusot() -> SemanticContextKey {
    SemanticContextKey::enabling(["creusot"])
}

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    std::fs::write(root.path().join("src/far.rs"), FAR).into_diagnostic()?;
    Ok(root)
}

fn node(graph: &CrateIr, name: &str) -> Result<NodeId> {
    graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .find(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id())
        .ok_or_else(|| miette::miette!("the library declares `{name}`"))
}

fn session_of(root: &Path, contexts: Vec<SemanticContextKey>) -> Result<(SourceDatabase, CrateIr)> {
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(contexts)
        .with_cfg_planning(CfgPlanning::Explicit);
    let mut session = SourceDatabase::load_with(root, &options).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok((session, graph))
}

/// What the only call `caller` makes in `context` names, described.
fn callee_in(
    session: &SourceDatabase,
    graph: &CrateIr,
    caller: &str,
    context: &SemanticContextKey,
) -> Result<String> {
    let entries = graph
        .semantic_context_functions(node(graph, caller)?)
        .into_diagnostic()?;
    let entry = entries
        .iter()
        .find(|entry| entry.context() == context)
        .ok_or_else(|| miette::miette!("{context:?} has facts for `{caller}`"))?;
    let [call] = &entry.function().calls()[..] else {
        return Err(miette::miette!("`{caller}` makes one call"));
    };
    let SemanticCallTarget::Function(target) = call.target() else {
        return Err(miette::miette!("the call names a function: {call:?}"));
    };
    Ok(session.describe_item(*target))
}

/// The description of the declaration `harness` calls in the kani context, which exists only
/// there, in a database that loaded `contexts`.
fn gated_description(root: &Path, contexts: Vec<SemanticContextKey>) -> Result<String> {
    let (session, graph) = session_of(root, contexts)?;
    callee_in(&session, &graph, "harness", &kani())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_declaration_only_a_context_has_is_not_described_by_that_contexts_copy() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let description = gated_description(root.path(), vec![kani()])?;
    assert!(
        !description.contains("__cordial_context_"),
        "the description names a context's copy, which means nothing in another run: {description}"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_same_declaration_has_one_description_whichever_context_number_it_is_read_in() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let alone = gated_description(root.path(), vec![kani()])?;
    // `creusot` sorts first, so kani is the second context of this run.
    let second = gated_description(root.path(), vec![creusot(), kani()])?;
    assert_eq!(
        alone, second,
        "one declaration, read as context 0 and as context 1"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn exclusive_declarations_of_one_name_stay_two_identities() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let (session, graph) = session_of(root.path(), vec![creusot(), kani()])?;
    let in_kani = callee_in(&session, &graph, "kani_caller", &kani())?;
    let in_creusot = callee_in(&session, &graph, "creusot_caller", &creusot())?;
    assert_ne!(
        in_kani, in_creusot,
        "two declarations, each in a context of its own"
    );
    for description in [&in_kani, &in_creusot] {
        assert!(!description.contains("__cordial_context_"), "{description}");
    }
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_gated_callee_in_a_file_the_projection_is_not_asked_about_is_still_located() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let (_, graph) = session_of(root.path(), vec![kani()])?;
    let external = graph
        .external_calls(node(&graph, "reaches")?)
        .into_diagnostic()?;
    let [call] = &external[..] else {
        return Err(miette::miette!("one external call: {external:?}"));
    };
    assert_eq!(call.context(), &Some(kani()));
    assert!(
        call.callee().file().ends_with("src/far.rs"),
        "the callee is where `away` is written: {call:?}"
    );
    Ok(())
}

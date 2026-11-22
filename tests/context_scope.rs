//! Cfg contexts are about the workspace's own packages. A path dependency is local to
//! rust-analyzer (it has no registry source), but it is not analysed: a gate in its files is
//! not a reason to rebuild the project under a new cfg, and its crates are not copied for a
//! context, which would rebuild them for nothing.

use cordial::{
    CrateIr, CrateTarget, IrView, SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const APP: &str = "\
pub fn base() {}

#[cfg(kani)]
pub fn gated() { base(); }
";

const DEPENDENCY: &str = "\
pub fn plain() {}

#[cfg(only_in_the_dependency)]
pub fn gated_there() {}

#[cfg(all(windows, miri))]
pub fn elsewhere() {}
";

/// A package with one gate of its own and a path dependency that has gates the package never
/// mentions. The package is not a workspace (no `[workspace]` table), so the dependency is not a
/// member either.
fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src", "dependency/src"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n\
         [dependencies]\ndependency = { path = 'dependency' }\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), APP).into_diagnostic()?;
    std::fs::write(
        root.path().join("dependency/Cargo.toml"),
        "[package]\nname='dependency'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("dependency/src/lib.rs"), DEPENDENCY).into_diagnostic()?;
    Ok(root)
}

fn project(root: &Path) -> Result<(SourceDatabase, CrateIr)> {
    let mut session =
        SourceDatabase::load_with(root, &SemanticCargoOptions::default()).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("app", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok((session, graph))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_gate_only_a_path_dependency_has_asks_for_no_context() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let (_, graph) = project(root.path())?;
    let plan = graph
        .semantic_context_plan()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("a plan was made"))?;
    assert_eq!(
        plan.contexts(),
        &[SemanticContextKey::enabling(["kani"])],
        "the package's own gate is the only one asked for"
    );
    assert!(*plan.complete());
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_context_copies_the_packages_crates_and_not_a_path_dependencys() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let (mut session, _) = project(root.path())?;
    let added = session
        .add_context_crates(&SemanticContextKey::enabling(["kani"]))
        .into_diagnostic()?;
    // The package's library is the only crate it owns.
    assert_eq!(added.crate_count(), 1);
    Ok(())
}

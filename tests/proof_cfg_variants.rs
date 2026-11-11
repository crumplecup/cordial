//! Verifier-gated code is analysed in a second cfg context inside the one database.

use cordial::{CrateTarget, IrView, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const LIBRARY: &str = "\
pub fn helper() -> Result<(), std::io::Error> { Ok(()) }

#[cfg(kani)]
pub fn harness() { let _ = helper(); }
";

fn fixture() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    Ok(root)
}

/// Cfg sets of every context entry for the fixture's library root.
fn library_contexts(session: &mut SourceDatabase, root: &Path) -> Result<Vec<Vec<String>>> {
    let graph = session
        .project(&CrateTarget::new("gated", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    let snapshot = graph
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing semantic snapshot"))?;
    let library = root.canonicalize().into_diagnostic()?.join("src/lib.rs");
    Ok(snapshot
        .workspace()
        .crates()
        .iter()
        .filter(|krate| krate.root() == &library)
        .map(|krate| krate.cfg().clone())
        .collect())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn ordinary_load_has_one_context_per_workspace_crate() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let contexts = library_contexts(&mut session, root.path())?;
    assert_eq!(contexts.len(), 1, "{contexts:?}");
    assert!(!contexts[0].iter().any(|atom| atom == "kani"));
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn proof_cfgs_add_a_variant_without_changing_the_base_context() -> Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let options = SemanticCargoOptions::default().with_proof_cfgs(vec!["kani".into()]);
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let contexts = library_contexts(&mut session, root.path())?;
    assert_eq!(contexts.len(), 2, "{contexts:?}");
    let proof = contexts
        .iter()
        .filter(|cfg| cfg.iter().any(|atom| atom == "kani"))
        .count();
    assert_eq!(proof, 1, "exactly one variant enables kani: {contexts:?}");
    Ok(())
}

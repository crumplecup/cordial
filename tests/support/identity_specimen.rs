//! A small crate whose `caller` calls definitions of every kind of origin: two functions
//! named `alpha` in different files, and two `helper`s two macro invocations generate.

use std::path::{Path, PathBuf};

use cordial::{
    CrateIr, CrateTarget, IrView, QueryBuilder, SemanticCallTarget, SemanticItemId,
    SemanticReturnContract, SemanticType, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

pub const LIB: &str = r#"pub mod first;
pub mod second;

pub struct Widget;
pub enum Choice { Left, Right }

macro_rules! helper {
    () => { pub fn helper() {} };
}
pub mod made { helper!(); }
pub mod also { helper!(); }

pub fn caller() -> Result<Widget, Choice> {
    made::helper();
    also::helper();
    first::alpha();
    second::alpha();
    Ok(Widget)
}
"#;

pub fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"identity_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (file, text) in [
        ("lib.rs", LIB),
        ("first.rs", "pub fn alpha() {}\n"),
        ("second.rs", "pub fn alpha() {}\n"),
    ] {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    Ok(root)
}

pub fn paths(root: &Path) -> Vec<PathBuf> {
    ["lib.rs", "first.rs", "second.rs"]
        .map(|file| root.join("src").join(file))
        .into()
}

pub fn project(session: &mut SourceDatabase, root: &Path) -> Result<CrateIr> {
    project_in_order(session, root, false)
}

/// Project the files in the order given, or reversed: the order definitions are
/// interned in follows it.
pub fn project_in_order(
    session: &mut SourceDatabase,
    root: &Path,
    reversed: bool,
) -> Result<CrateIr> {
    let mut files = paths(root);
    if reversed {
        files.reverse();
    }
    session
        .project(&CrateTarget::new("identity_specimen", root), &files)
        .into_diagnostic()
}

/// What the graph says `caller` calls and returns, as identities.
pub fn caller_identities(graph: &CrateIr) -> Result<(Vec<SemanticItemId>, Vec<SemanticItemId>)> {
    let query = QueryBuilder::new()
        .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
        .build();
    for node in graph.nodes_matching(&query) {
        let Some(facts) = graph.semantic_function(node.id()).into_diagnostic()? else {
            continue;
        };
        if facts.label() != "caller" {
            continue;
        }
        let targets = facts
            .calls()
            .iter()
            .filter_map(|call| match call.target() {
                SemanticCallTarget::Function(id) => Some(*id),
                _ => None,
            })
            .collect();
        let mut types = Vec::new();
        if let SemanticReturnContract::Result { success, error, .. } = facts.output() {
            for ty in [success, error] {
                if let SemanticType::Adt { definition, .. } = ty {
                    types.push(*definition);
                }
            }
        }
        return Ok((targets, types));
    }
    Err(miette::miette!("no facts for caller"))
}

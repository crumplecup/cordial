//! A definition's identity is a description of where it is written, so it means the same in
//! every database: built twice, in different orders, the same definitions get the same
//! descriptions, two definitions never share one, and a definition a macro generated is
//! described by the call that produced it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cordial::{
    CrateIr, CrateTarget, IrView, QueryBuilder, SemanticCallTarget, SemanticItemId,
    SemanticReturnContract, SemanticType, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

const LIB: &str = r#"pub mod first;
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

fn specimen() -> Result<tempfile::TempDir> {
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

fn paths(root: &Path) -> Vec<PathBuf> {
    ["lib.rs", "first.rs", "second.rs"]
        .map(|file| root.join("src").join(file))
        .into()
}

fn project(session: &mut SourceDatabase, root: &Path) -> Result<CrateIr> {
    project_in_order(session, root, false)
}

/// Project the files in the order given, or reversed: the order definitions are
/// interned in follows it.
fn project_in_order(session: &mut SourceDatabase, root: &Path, reversed: bool) -> Result<CrateIr> {
    let mut files = paths(root);
    if reversed {
        files.reverse();
    }
    session
        .project(&CrateTarget::new("identity_specimen", root), &files)
        .into_diagnostic()
}

/// What the graph says `caller` calls and returns, as identities.
fn caller_identities(graph: &CrateIr) -> Result<(Vec<SemanticItemId>, Vec<SemanticItemId>)> {
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

/// The descriptions, by what they describe, from one database built from the specimen.
fn described(root: &Path, reversed: bool) -> Result<BTreeMap<String, String>> {
    let mut session = SourceDatabase::load(root).into_diagnostic()?;
    let graph = project_in_order(&mut session, root, reversed)?;
    let (targets, types) = caller_identities(&graph)?;
    let mut found = BTreeMap::new();
    for (position, id) in targets.iter().chain(&types).enumerate() {
        found.insert(format!("#{position}"), session.describe_item(*id));
    }
    Ok(found)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn the_same_definitions_get_the_same_descriptions_in_a_separate_database() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let first = described(root.path(), false)?;
    // The same project in a separate database that interns the files in the other order.
    let second = described(root.path(), true)?;
    assert!(!first.is_empty(), "the caller's facts name its callees");
    assert_eq!(first, second, "the descriptions of the same definitions");
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn descriptions_tell_apart_definitions_with_one_name() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = project(&mut session, root.path())?;
    let (targets, _) = caller_identities(&graph)?;
    let descriptions: Vec<String> = targets
        .iter()
        .map(|id| session.describe_item(*id))
        .collect();
    assert_eq!(descriptions.len(), 4, "four callees: {descriptions:?}");
    let mut distinct = descriptions.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        descriptions.len(),
        "`alpha` in two files and `helper` from two modules are four definitions: {descriptions:?}"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn a_definition_with_a_site_is_found_again_and_one_without_is_not() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let mut session = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = project(&mut session, root.path())?;
    let (targets, types) = caller_identities(&graph)?;
    let mut generated = 0;
    for id in targets.iter().chain(&types) {
        let description = session.describe_item(*id);
        // The fourth field is the chain of expansions a generated definition came through.
        let from_macro = description
            .split('|')
            .nth(3)
            .is_some_and(|chain| !chain.is_empty());
        generated += usize::from(from_macro);
        assert_eq!(
            session.knows_item(*id),
            !from_macro,
            "{description}: found again exactly when it is written in a file"
        );
    }
    assert_eq!(generated, 2, "the two `helper`s come from macro calls");
    assert_eq!(types.len(), 2, "Widget and Choice");
    Ok(())
}

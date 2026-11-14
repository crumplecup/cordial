//! A definition's identity is a description of where it is written, so it means the same in
//! every database: built twice, in different orders, the same definitions get the same
//! descriptions, two definitions never share one, and a definition a macro generated is
//! described by the call that produced it.

#[path = "support/identity_specimen.rs"]
mod identity_specimen;

use std::collections::BTreeMap;
use std::path::Path;

use cordial::SourceDatabase;
use identity_specimen::{caller_identities, project, project_in_order, specimen};
use miette::{IntoDiagnostic, Result};

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

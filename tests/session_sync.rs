//! A loaded database follows the files on disk: plain source edits are fed in as
//! input changes and agree with a fresh load; anything else asks for a reload.

mod support {
    pub mod mini_workspace;
}
use cordial::{CrateIr, CrateTarget, IrView, SourceDatabase, SyncOutcome};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::path::Path;

#[tracing::instrument(level = "debug", skip(session), err(level = "warn"))]
fn project(session: &mut SourceDatabase, root: &Path) -> Result<CrateIr> {
    session
        .project(
            &CrateTarget::new("mini", root),
            &[root.join("src/lib.rs"), root.join("src/other.rs")],
        )
        .into_diagnostic()
}

/// Drop the database's interned ids (a function's `definition` and the function
/// a call's `target` names), which are not stable between two databases and say
/// nothing about what the code means: they follow the order the database
/// happened to evaluate functions in, and several threads evaluate them.
fn without_ids(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("definition");
            if let Some(serde_json::Value::Object(target)) = map.get_mut("target")
                && let Some(id) = target.get_mut("Function")
            {
                *id = serde_json::Value::from(0);
            }
            map.values_mut().for_each(without_ids);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(without_ids),
        _ => {}
    }
}

/// Every function's facts by label: what a stale database would get wrong.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn facts(session: &mut SourceDatabase, root: &Path) -> Result<BTreeMap<String, serde_json::Value>> {
    let graph = project(session, root)?;
    let mut by_label = BTreeMap::new();
    for node in graph.nodes_matching(
        &cordial::QueryBuilder::new()
            .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
            .build(),
    ) {
        if let Some(function) = graph.semantic_function(node.id()).into_diagnostic()? {
            let mut value = serde_json::to_value(&function).into_diagnostic()?;
            without_ids(&mut value);
            by_label.insert(function.label().to_string(), value);
        }
    }
    Ok(by_label)
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_agrees_with_a_fresh_load(
    session: &mut SourceDatabase,
    root: &Path,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let followed = facts(session, root)?;
    let mut fresh = SourceDatabase::load(root).into_diagnostic()?;
    let expected = facts(&mut fresh, root)?;
    for (label, fact) in &followed {
        assert_eq!(
            Some(fact),
            expected.get(label),
            "{label}: the synced session is stale"
        );
    }
    assert_eq!(
        followed.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    Ok(followed)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn source_edits_are_followed_and_everything_else_asks_for_a_reload() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let root = scratch.path();
    support::mini_workspace::write(root)?;
    let lib = root.join("src/lib.rs");
    let other = root.join("src/other.rs");
    let (lib_text, other_text) = (
        std::fs::read_to_string(&lib).into_diagnostic()?,
        std::fs::read_to_string(&other).into_diagnostic()?,
    );
    let mut session = SourceDatabase::load(root).into_diagnostic()?;
    let before = facts(&mut session, root)?;
    assert_eq!(
        session.sync_from_disk().into_diagnostic()?,
        SyncOutcome::Current
    );

    // A comment line above everything moves every item and changes no fact.
    std::fs::write(&lib, format!("// a comment\n{lib_text}")).into_diagnostic()?;
    assert_eq!(
        session.sync_from_disk().into_diagnostic()?,
        SyncOutcome::Updated(vec![lib.canonicalize().into_diagnostic()?])
    );
    let commented = assert_agrees_with_a_fresh_load(&mut session, root)?;
    assert_eq!(
        commented.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );

    // A body edit in one file changes that function's facts and its caller's.
    std::fs::write(&other, "pub fn leaf() -> u16 {\n    1\n}\n").into_diagnostic()?;
    session.sync_from_disk().into_diagnostic()?;
    let retyped = assert_agrees_with_a_fresh_load(&mut session, root)?;
    assert_ne!(retyped["leaf"], before["leaf"], "the edit reached leaf");
    assert_ne!(
        retyped["top"], before["top"],
        "and the caller that reads leaf"
    );

    // A new function is a fact the old session did not have.
    std::fs::write(
        &other,
        format!("{other_text}pub fn added() -> u8 {{ 3 }}\n"),
    )
    .into_diagnostic()?;
    session.sync_from_disk().into_diagnostic()?;
    let added = assert_agrees_with_a_fresh_load(&mut session, root)?;
    assert!(added.contains_key("added"), "{:?}", added.keys());

    // Back to the original text: followed again, and equal to the first facts.
    std::fs::write(&lib, &lib_text).into_diagnostic()?;
    std::fs::write(&other, &other_text).into_diagnostic()?;
    session.sync_from_disk().into_diagnostic()?;
    assert_eq!(assert_agrees_with_a_fresh_load(&mut session, root)?, before);

    // A source file the database never loaded needs a reload.
    let extra = root.join("src/extra.rs");
    std::fs::write(&extra, "pub fn extra() {}\n").into_diagnostic()?;
    assert!(matches!(
        session.sync_from_disk().into_diagnostic()?,
        SyncOutcome::Reload(path) if path == extra
    ));
    std::fs::remove_file(&extra).into_diagnostic()?;

    // So does a change to what the Cargo graph was built from.
    let manifest = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).into_diagnostic()?;
    std::fs::write(&manifest, format!("{text}\n# changed\n")).into_diagnostic()?;
    assert!(matches!(
        session.sync_from_disk().into_diagnostic()?,
        SyncOutcome::Reload(path) if path.ends_with("Cargo.toml")
    ));
    std::fs::write(&manifest, text).into_diagnostic()?;
    assert_eq!(
        session.sync_from_disk().into_diagnostic()?,
        SyncOutcome::Current
    );
    Ok(())
}

//! `#[instrument]` is the macro the attribute's path resolves to, not a spelling:
//! an import under another name is it, and an unresolved `instrument` is not.

use cordial::IrView;
use miette::{IntoDiagnostic, Result};

#[path = "support/projected_crate.rs"]
mod projected_crate;

const LIBRARY: &str = include_str!("fixtures/instrument_identity/lib.rs.txt");

fn instrumented() -> Result<Vec<String>> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='identity'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ntracing='0.1'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let graph = projected_crate::projected_crate(root.path())?;
    let mut names = Vec::new();
    for node in graph.nodes_matching(&cordial::BasicQuery::all_nodes()) {
        let Some(attributes) = graph.source_file_attributes(node.id()).into_diagnostic()? else {
            continue;
        };
        for item in attributes.items() {
            if item
                .attributes()
                .iter()
                .any(|attribute| attribute.instrument())
            {
                names.push(item.path().clone());
            }
        }
    }
    names.sort();
    Ok(names)
}

#[test]
fn an_alias_and_the_written_path_are_instrument_and_an_unresolved_name_is_not() -> Result<()> {
    cordial::init_tracing();
    assert_eq!(instrumented()?, ["renamed", "written_out"]);
    Ok(())
}

/// Each function of the library, with whether its attributes say it is instrumented and whether that could not be decided.
fn instrument_states(library: &str) -> Result<std::collections::BTreeMap<String, (bool, bool)>> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='identity'\nversion='0.1.0'\nedition='2024'\n[features]\noff = []\n[dependencies]\ntracing='0.1'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    let graph = projected_crate::projected_crate(root.path())?;
    let mut states = std::collections::BTreeMap::new();
    for node in graph.nodes_matching(&cordial::BasicQuery::all_nodes()) {
        let Some(attributes) = graph.source_file_attributes(node.id()).into_diagnostic()? else {
            continue;
        };
        for item in attributes.items() {
            let instrument = item.attributes().iter().any(|attribute| attribute.instrument());
            let unresolved = item.attributes().iter().any(|attribute| attribute.instrument_unresolved());
            states.insert(item.path().clone(), (instrument, unresolved));
        }
    }
    Ok(states)
}

/// What an attribute path names is not decided where the code has no scope: an item whose `cfg` guard is off is
/// unresolved, and is not the same as a path that resolves to something that is not `instrument`.
#[test]
fn an_attribute_in_code_with_no_scope_is_unresolved_and_not_a_path_that_names_something_else() -> Result<()> {
    cordial::init_tracing();
    let states = instrument_states(
        "#[cfg(feature = \"off\")]\n#[tracing::instrument]\npub fn guarded() {}\n\n\
         #[tracing::instrument]\npub fn written_out() {}\n\n\
         #[instrument]\npub fn imported_nowhere() {}\n\n\
         #[inline]\npub fn other_attribute() {}\n",
    )?;
    assert_eq!(states.get("written_out"), Some(&(true, false)), "{states:?}");
    assert_eq!(states.get("imported_nowhere"), Some(&(false, false)), "{states:?}");
    assert_eq!(states.get("other_attribute"), Some(&(false, false)), "{states:?}");
    assert_eq!(states.get("guarded"), Some(&(false, true)), "the guard is off, so it is not decided: {states:?}");
    Ok(())
}

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

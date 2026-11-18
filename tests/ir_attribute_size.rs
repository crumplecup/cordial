//! A function's facts are held in the graph as a JSON value, which is many small allocations (a map
//! for each object, a string for each key). Encoded compactly they are a fraction of that, which is
//! what makes packing the bulk attributes worth the change to how they are read.

use cordial::{
    ATTR_SEMANTIC_FUNCTION, BasicQuery, CrateTarget, IrView, SemanticCargoOptions, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

/// What a JSON value holds in memory, close enough to compare with an encoding: the enum itself for
/// each value, and for each string, array and object what it allocates (a key is a string; a map
/// entry is a node in a tree).
fn held_bytes(value: &serde_json::Value) -> usize {
    const VALUE: usize = std::mem::size_of::<serde_json::Value>();
    const STRING: usize = std::mem::size_of::<String>();
    // A B-tree map's per-entry cost, which is a key, a value and the node's share of the rest.
    const ENTRY: usize = 24;
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            VALUE
        }
        serde_json::Value::String(text) => VALUE + text.capacity(),
        serde_json::Value::Array(items) => {
            VALUE
                + items.capacity() * VALUE
                + items
                    .iter()
                    .map(held_bytes)
                    .map(|b| b - VALUE)
                    .sum::<usize>()
        }
        serde_json::Value::Object(map) => {
            VALUE
                + map
                    .iter()
                    .map(|(key, value)| STRING + key.capacity() + ENTRY + held_bytes(value))
                    .sum::<usize>()
        }
    }
}

#[test]
fn a_functions_facts_are_a_fraction_of_the_size_as_a_json_value_when_encoded() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='sized'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..20)
        .map(|index| {
            format!("pub fn f{index}(x: Option<usize>) -> Result<usize, String> {{ x.ok_or(String::from(\"none\")).map(|v| v + {index}) }}\n")
        })
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    let mut session =
        SourceDatabase::load_saved(root.path(), &SemanticCargoOptions::default(), None)
            .into_diagnostic()?;
    let graph = session
        .project(
            &CrateTarget::new("sized", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;

    let (mut held, mut encoded, mut functions) = (0usize, 0usize, 0usize);
    for node in graph.nodes_matching(&BasicQuery::items()) {
        let Some(value) = graph
            .node(node.id())
            .and_then(|node| node.attr(ATTR_SEMANTIC_FUNCTION))
        else {
            continue;
        };
        held += held_bytes(value);
        encoded += rmp_serde::to_vec(value).into_diagnostic()?.len();
        functions += 1;
    }
    assert!(
        functions >= 20,
        "the facts of the functions were found: {functions}"
    );
    assert!(
        encoded * 3 <= held,
        "encoded they are at most a third of what the values hold: {encoded} bytes against {held} for {functions} functions"
    );
    Ok(())
}

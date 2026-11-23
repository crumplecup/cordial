//! The texts of library files (dependencies, the sysroot) leave memory under pressure and are read
//! from the disk when something asks, and what the analysis finds is the same.

use cordial::{CrateTarget, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};

/// A graph's text without the order a hash map was printed in: what comes before its indexes as it
/// is, and the words of the indexes sorted.
fn in_order(graph: &str) -> (String, Vec<String>) {
    let (nodes, indexes) = graph.split_once(" indexes: ").unwrap_or((graph, ""));
    let mut words: Vec<String> = indexes
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    words.sort();
    (nodes.to_owned(), words)
}

/// A graph and the texts in memory before and after the drop, as `(files, bytes)`.
struct Run {
    graph: String,
    before: (usize, usize),
    after: (usize, usize),
}

fn graph_of(root: &std::path::Path, drop_first: bool) -> Result<Run> {
    let mut session = SourceDatabase::load_saved(root, &SemanticCargoOptions::default(), None)
        .into_diagnostic()?;
    let before = session.resident_texts();
    if drop_first {
        let (files, bytes) = session.drop_library_texts();
        assert!(
            files > 0 && bytes > 0,
            "library texts left memory: {files} files, {bytes} bytes"
        );
    }
    let after = session.resident_texts();
    let graph = session
        .project(&CrateTarget::new("cold", root), &[root.join("src/lib.rs")])
        .into_diagnostic()?;
    Ok(Run {
        graph: format!("{graph:?}"),
        before,
        after,
    })
}

#[test]
fn library_texts_leave_memory_and_are_read_again_and_the_graph_is_the_same() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='cold'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..20)
        .map(|index| {
            // Calls nothing outside the crate: the ids of library functions depend on the order
            // parallel priming interns them, and would differ between any two sessions.
            format!("pub fn f{index}() -> usize {{ {index} + 1 }}\n")
        })
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;

    let kept = graph_of(root.path(), false)?;
    let dropped = graph_of(root.path(), true)?;

    assert!(
        dropped.before.1 > 1_000_000,
        "the library texts are in memory to begin with: {:?}",
        dropped.before
    );
    assert!(
        dropped.after.1 * 4 < dropped.before.1,
        "most of the bytes left memory: {:?} then {:?}",
        dropped.before,
        dropped.after
    );
    assert!(
        kept.before.1 > 1_000_000 && kept.after == kept.before,
        "and nothing left without the drop"
    );
    let (dropped_nodes, dropped_words) = in_order(&dropped.graph);
    let (kept_nodes, kept_words) = in_order(&kept.graph);
    let at = dropped_nodes
        .bytes()
        .zip(kept_nodes.bytes())
        .position(|(left, right)| left != right);
    let around = |text: &str| {
        let from = at.unwrap_or(0).saturating_sub(150);
        text.get(from..(at.unwrap_or(0) + 250).min(text.len()))
            .unwrap_or("")
            .to_owned()
    };
    assert!(
        dropped_nodes == kept_nodes,
        "the nodes are the ones without the drop; first difference at {at:?}:\n  with: {}\n  without: {}",
        around(&dropped_nodes),
        around(&kept_nodes)
    );
    let only_dropped: Vec<&String> = dropped_words
        .iter()
        .filter(|word| !kept_words.contains(word))
        .collect();
    let only_kept: Vec<&String> = kept_words
        .iter()
        .filter(|word| !dropped_words.contains(word))
        .collect();
    assert!(
        dropped_words == kept_words,
        "and so are the indexes: only with the drop {only_dropped:?}, only without {only_kept:?}"
    );
    Ok(())
}

/// The manifest of a virtual workspace is in the VFS with a text and no source root holds it (it is
/// not Rust, and no package's directory is the root of the workspace): walking the files of the
/// library roots has to go past it, not ask it for a root it does not have.
#[test]
fn a_file_that_no_source_root_holds_does_not_stop_the_library_texts_being_found() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("member/src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers=['member']\nresolver='2'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("member/Cargo.toml"),
        "[package]\nname='member'\nversion='0.1.0'\nedition='2024'\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("member/src/lib.rs"),
        "pub fn caller() -> usize { 1 }\n",
    )
    .into_diagnostic()?;

    let mut session =
        SourceDatabase::load_saved(root.path(), &SemanticCargoOptions::default(), None)
            .into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("member", root.path().join("member")),
            &[root.path().join("member/src/lib.rs")],
        )
        .into_diagnostic()?;
    let (files, _) = session.drop_library_texts();
    assert!(files > 0, "the sysroot's texts are still found: {files}");
    Ok(())
}

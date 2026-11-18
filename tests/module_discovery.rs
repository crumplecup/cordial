//! The files of a crate are found by following its `mod` declarations, through the files the
//! database holds.
//!
//! What a declaration may lead to is read from the syntax of the file that has it and kept with the
//! file. Which of those files exists is a question of the files the database holds, so that a file
//! that appears after a run changes what is found even where the declaring file did not change.
#![cfg(feature = "internal_error_chain")]

use cordial::{CfgPlanning, CrateTarget, IrView, SavedState, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::{Path, PathBuf};

/// A crate with each kind of declaration: a file beside the declaring one, a directory with a
/// `mod.rs`, an explicit path, an inline module with a file inside it, a declaration with no file, a
/// module a configuration excludes, and a file no declaration leads to.
fn crate_with_every_declaration() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    let write = |path: &str, text: &str| -> Result<()> {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).into_diagnostic()?;
        std::fs::write(path, text).into_diagnostic()
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"modules\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
    )?;
    write(
        "src/lib.rs",
        "mod a;\nmod dir;\n#[path = \"custom/z.rs\"]\nmod z;\nmod inline {\n    mod deep;\n}\n#[cfg(test)]\nmod tests;\nmod missing;\n",
    )?;
    write("src/a.rs", "// a\n")?;
    write("src/dir/mod.rs", "mod child;\n")?;
    write("src/dir/child.rs", "// child\n")?;
    write("src/custom/z.rs", "// z\n")?;
    write("src/inline/deep.rs", "// deep\n")?;
    write("src/tests.rs", "// excluded by its configuration\n")?;
    write("src/orphan.rs", "// declared by nothing\n")?;
    Ok(root)
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root.join("src"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .map(walkdir::DirEntry::into_path)
        .collect()
}

/// The files the architecture step finds, relative to `src`, sorted.
fn found(root: &Path, saved: &SavedState) -> Result<Vec<String>> {
    cordial::init_tracing();
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Explicit);
    let mut session =
        SourceDatabase::load_saved(root, &options, Some(saved.clone())).into_diagnostic()?;
    let graph = session
        .project(&CrateTarget::new("modules", root), &rust_files(root))
        .into_diagnostic()?;
    let inventory = graph.source_error_architecture().into_diagnostic()?;
    let src = root.canonicalize().into_diagnostic()?.join("src");
    let mut files: Vec<String> = inventory
        .files()
        .iter()
        .map(|file| {
            file.path()
                .strip_prefix(&src)
                .unwrap_or(file.path())
                .display()
                .to_string()
        })
        .collect();
    files.sort();
    Ok(files)
}

#[test]
fn the_files_of_a_crate_are_the_ones_its_declarations_lead_to_and_a_new_file_is_found() -> Result<()>
{
    let root = crate_with_every_declaration()?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));

    let first = found(root.path(), &saved)?;
    assert_eq!(
        first,
        [
            "a.rs",
            "custom/z.rs",
            "dir/child.rs",
            "dir/mod.rs",
            "inline/deep.rs",
            "lib.rs"
        ],
        "a declaration with no file, an excluded module and an undeclared file are not followed"
    );

    // A file appears for the declaration that had none. The file that declares it is as it was, and
    // the run restores what the last one saved.
    std::fs::write(root.path().join("src/missing.rs"), "// now there\n").into_diagnostic()?;
    let second = found(root.path(), &saved)?;
    assert!(
        second.contains(&"missing.rs".to_owned()),
        "a file that appeared for a declaration is not found: {second:?}"
    );
    assert_eq!(second.len(), first.len() + 1, "{second:?}");
    Ok(())
}

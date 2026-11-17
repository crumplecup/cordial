//! A crate of many files, each a large literal, so that a file's syntax tree is about a megabyte
//! and there are more files than the capacity of the cache of parses (128).

use miette::{IntoDiagnostic, Result};
use std::path::{Path, PathBuf};

/// The crate: `files` files, each with a table of `elements` numbers and a function that reads it.
pub fn crate_of_large_files(
    name: &str,
    files: usize,
    elements: usize,
) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .into_diagnostic()?;
    let table = (0..elements)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut library = String::new();
    for file in 0..files {
        library.push_str(&format!("pub mod file{file};\n"));
        std::fs::write(
            root.path().join(format!("src/file{file}.rs")),
            format!(
                "pub static TABLE{file}: [u32; {elements}] = [{table}];\npub fn f{file}() -> u32 {{ TABLE{file}[1] }}\n"
            ),
        )
        .into_diagnostic()?;
    }
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    Ok(root)
}

/// The files under `src` of the crate at `root`, sorted.
pub fn sources_of(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    found.sort();
    found
}

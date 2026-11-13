//! A crate of many modules whose functions call down a chain of them, big enough
//! that what a run does is visible in its times.

use miette::{IntoDiagnostic, Result};
use std::path::Path;

/// Write `files` modules of `per_file` functions at `root`. Function `j` of module
/// `i` calls function `j` of module `i + 1`; the last module's functions end the
/// chains. Every call crosses a file, so one edit reaches every caller above it.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub(crate) fn write(root: &Path, files: usize, per_file: usize) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'mini'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let modules: String = (0..files).map(|i| format!("pub mod m{i};\n")).collect();
    std::fs::write(root.join("src/lib.rs"), modules).into_diagnostic()?;
    for i in 0..files {
        let body: String = (0..per_file)
            .map(|j| {
                let call = if i + 1 < files {
                    format!("crate::m{}::f{}_{j}(x)", i + 1, i + 1)
                } else {
                    "x".to_owned()
                };
                format!("pub fn f{i}_{j}(x: u32) -> u32 {{\n    {call} + {j}\n}}\n\n")
            })
            .collect();
        std::fs::write(root.join(format!("src/m{i}.rs")), body).into_diagnostic()?;
    }
    Ok(())
}

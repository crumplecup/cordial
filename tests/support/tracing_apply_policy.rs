//! A multi-crate workspace on disk with its own `cordial.toml` and checklist, for the tracing apply
//! tests that need a dependency graph or a `#[path]` splice.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};

/// A workspace with one or more crates, its own `cordial.toml`, and a
/// checklist -- for verifier-policy tests, where a single synthetic
/// crate (as [`write_apply_fixture`] builds) isn't enough to exercise
/// dependency-graph or `#[path]`-splice propagation.
pub struct PolicyFixture {
    pub workspace: PathBuf,
    pub checklist: PathBuf,
    pub _temp: tempfile::TempDir,
}

/// Write one workspace member crate. `deps` are `(dependency crate
/// name, relative path to it from this crate's own directory)` pairs,
/// written as real Cargo path dependencies.
pub fn write_policy_crate(
    workspace: &Path,
    name: &str,
    source: &str,
    deps: &[(&str, &str)],
) -> miette::Result<()> {
    let crate_root = workspace.join(name);
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(src_root.join("lib.rs"), source)
        .into_diagnostic()
        .wrap_err("write source")?;
    let mut manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
    if !deps.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (dep_name, dep_rel_path) in deps {
            manifest.push_str(&format!("{dep_name} = {{ path = \"{dep_rel_path}\" }}\n"));
        }
    }
    fs::write(crate_root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("crate manifest")?;
    Ok(())
}

pub fn write_policy_fixture(
    members: &[&str],
    cordial_toml: &str,
    checklist: &str,
) -> miette::Result<PolicyFixture> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace)
        .into_diagnostic()
        .wrap_err("workspace dir")?;
    let members_list = members
        .iter()
        .map(|member| format!("\"{member}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        workspace.join("Cargo.toml"),
        format!("[workspace]\nmembers = [{members_list}]\nresolver = \"2\"\n"),
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    fs::write(workspace.join("cordial.toml"), cordial_toml)
        .into_diagnostic()
        .wrap_err("cordial.toml")?;
    let checklist_path = temp.path().join("tracing-instrument.checklist.md");
    fs::write(&checklist_path, checklist)
        .into_diagnostic()
        .wrap_err("write checklist")?;
    Ok(PolicyFixture {
        workspace,
        checklist: checklist_path,
        _temp: temp,
    })
}

//! Shared executable-specimen lifecycle; production discovery supplies every fact.

use cordial::{CrateIr, IrView, PANICS_ETIQUETTE, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn specimen_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("main.rs")
}

pub fn copy_specimen(name: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir(fixture.path().join("src")).into_diagnostic()?;
    fs::copy(specimen_path(name), fixture.path().join("src/main.rs"))
        .into_diagnostic()
        .wrap_err("copy executable specimen into canonical binary source root")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"strategy-specimen\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[[bin]]\nname = \"specimen\"\npath = \"src/main.rs\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    Ok(fixture)
}

pub fn build(root: &Path) -> miette::Result<()> {
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "build",
            "--offline",
            "--quiet",
            "--bin",
            "specimen",
            "--target-dir",
        ])
        .arg(root.join("target"))
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "specimen must compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "specimen must compile without warnings"
    );
    Ok(())
}

pub fn execute(root: &Path, arguments: &[&str]) -> miette::Result<Output> {
    Command::new(root.join("target/debug/specimen"))
        .current_dir(root)
        .args(arguments)
        .output()
        .into_diagnostic()
}

pub fn discover(root: &Path) -> miette::Result<CrateIr> {
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(root)
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .with_ir_snapshot()
        .build();
    session.run(&RunAll).into_diagnostic()?;
    let cache = store.path().join("cache/strategy-specimen.ir.zst");
    let ir = CrateIr::read_cache(&cache).into_diagnostic()?;
    assert_eq!(
        ir.crate_kinds().into_diagnostic()?,
        [cordial::CrateKind::Bin]
    );
    Ok(ir)
}

pub fn function_facts(
    ir: &dyn IrView,
    name: &str,
) -> miette::Result<cordial::FunctionSourceSyntax> {
    let node = ir
        .node_by_path(name)
        .ok_or_else(|| miette::miette!("missing function {name}"))?;
    ir.function_source_syntax(node)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing function source facts for {name}"))
}

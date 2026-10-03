//! Cargo crate kinds: detected from Cargo metadata, recorded on the IR root.

use std::fs;
use std::path::Path;

use miette::{IntoDiagnostic, WrapErr};

use cordial::{
    ATTR_CRATE_KINDS, CrateIr, CrateKind, CrateTarget, IrMut, IrView, RunAll, SourceFile,
    SourceLoadView, discover_crate_targets,
};

fn write_crate(
    root: &Path,
    manifest_tail: &str,
    extra_files: &[(&str, &str)],
) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{manifest_tail}"),
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    for (name, body) in extra_files {
        fs::write(root.join(name), body)
            .into_diagnostic()
            .wrap_err("write source")?;
    }
    Ok(())
}

fn only_target(root: &Path) -> miette::Result<CrateTarget> {
    let mut targets = discover_crate_targets(root, &RunAll).into_diagnostic()?;
    assert_eq!(targets.len(), 1);
    Ok(targets.remove(0))
}

#[test]
fn proc_macro_lib_is_detected_from_cargo_metadata() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "\n[lib]\nproc-macro = true\n",
        &[("src/lib.rs", "")],
    )?;
    let target = only_target(fixture.path())?;
    assert_eq!(target.kinds(), &vec![CrateKind::ProcMacro]);
    assert!(target.is_proc_macro());
    Ok(())
}

#[test]
fn plain_lib_and_bin_are_not_proc_macros() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "",
        &[("src/lib.rs", ""), ("src/main.rs", "fn main() {}")],
    )?;
    let target = only_target(fixture.path())?;
    assert_eq!(target.kinds(), &vec![CrateKind::Lib, CrateKind::Bin]);
    assert!(!target.is_proc_macro());
    Ok(())
}

#[test]
fn synthetic_target_without_a_manifest_has_unknown_kinds() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    let target = only_target(fixture.path())?;
    assert!(target.kinds().is_empty());
    assert!(!target.is_proc_macro());
    Ok(())
}

fn populated_ir(kinds: Vec<CrateKind>) -> miette::Result<CrateIr> {
    let src_root = Path::new("/nonexistent/src").to_path_buf();
    let lib = SourceFile::new(src_root.join("lib.rs"), "pub fn f() {}".to_string());
    let view = SourceLoadView::new("fixture".to_string(), src_root, vec![lib], kinds);
    let mut ir = CrateIr::new("fixture");
    view.populate_ir(&mut ir).into_diagnostic()?;
    Ok(ir)
}

#[test]
fn source_loader_records_kinds_on_the_crate_root() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = populated_ir(vec![CrateKind::ProcMacro])?;
    let root = ir.root();
    let attr = ir
        .node_weight(root)
        .and_then(|weight| weight.attr(ATTR_CRATE_KINDS))
        .cloned();
    assert_eq!(attr, Some(serde_json::json!(["proc-macro"])));
    assert_eq!(
        IrView::crate_kinds(&ir).into_diagnostic()?,
        vec![CrateKind::ProcMacro]
    );
    assert!(IrView::is_proc_macro(&ir).into_diagnostic()?);
    Ok(())
}

#[test]
fn unknown_kinds_leave_the_root_unmarked() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = populated_ir(Vec::new())?;
    assert!(
        ir.node_weight(ir.root())
            .and_then(|weight| weight.attr(ATTR_CRATE_KINDS))
            .is_none()
    );
    assert!(IrView::crate_kinds(&ir).into_diagnostic()?.is_empty());
    assert!(!IrView::is_proc_macro(&ir).into_diagnostic()?);
    Ok(())
}

#[test]
fn crate_kinds_ignores_unrecognized_strings() -> miette::Result<()> {
    cordial::init_tracing();
    let mut ir = CrateIr::new("fixture");
    let root = ir.root();
    IrMut::set_attr(
        &mut ir,
        root,
        ATTR_CRATE_KINDS,
        serde_json::json!(["bin", "from-the-future", 7]),
    )
    .into_diagnostic()?;
    assert_eq!(
        IrView::crate_kinds(&ir).into_diagnostic()?,
        vec![CrateKind::Bin]
    );
    Ok(())
}

#[test]
fn crate_kind_strings_round_trip() {
    cordial::init_tracing();
    for kind in [CrateKind::Lib, CrateKind::Bin, CrateKind::ProcMacro] {
        assert_eq!(CrateKind::from_attr(kind.as_str()), Some(kind));
        assert_eq!(kind.to_string(), kind.as_str());
    }
    assert_eq!(CrateKind::from_attr("example"), None);
}

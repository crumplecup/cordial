use cordial::{CrateIr, SurrealGraphExport, surreal_statements};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn export_includes_root_node() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = CrateIr::new("demo");
    let export = SurrealGraphExport::from_crate_ir(&ir)
        .into_diagnostic()
        .wrap_err("snapshot")?;
    assert_eq!(export.crate_name, "demo");
    assert!(!export.nodes.is_empty());
    assert!(export.nodes[0].id.starts_with("demo:node:"));
    Ok(())
}

#[test]
fn surreal_statements_non_empty() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = CrateIr::new("demo");
    let export = SurrealGraphExport::from_crate_ir(&ir)
        .into_diagnostic()
        .wrap_err("snapshot")?;
    let statements = surreal_statements(&export);
    assert!(!statements.is_empty());
    assert!(statements[0].starts_with("CREATE demo:node:"));
    Ok(())
}

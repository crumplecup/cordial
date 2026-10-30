#![cfg(feature = "panics")]

use std::fs;
use std::path::Path;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts, specimen_path};

use cordial::{
    ATTR_FUNCTION_SOURCE_SYNTAX, BasicQuery, CrateIr, DeclaredFunctionOutput, EdgeKind, IrView,
    ItemKind, NodeKind,
};
use miette::IntoDiagnostic;
use sha2::{Digest, Sha256};

#[test]
fn result_panic_specimen_executes_and_has_shared_source_evidence() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_panic_result")?;
    build(fixture.path())?;
    let success = execute(fixture.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"8\n");
    let failure = execute(fixture.path(), &["fail"])?;
    assert!(!failure.status.success());
    assert!(failure.stdout.is_empty());
    let diagnostic = String::from_utf8(failure.stderr).into_diagnostic()?;
    assert!(diagnostic.contains("PermissionDenied"));
    assert!(diagnostic.contains("panicked"));

    let ir = discover(fixture.path())?;
    assert_functions(&ir, &["operation", "leaf", "caller", "main"])?;
    assert_panic_sites(&ir, "leaf", 1)?;
    let operation = function_facts(&ir, "operation")?;
    assert_eq!(
        operation.declared_output(),
        &DeclaredFunctionOutput::Explicit("std::result::Result<u32, std::io::Error>".into())
    );
    assert_eq!(
        function_facts(&ir, "leaf")?.declared_output(),
        &DeclaredFunctionOutput::Explicit("u32".into())
    );
    assert_source_snapshot(fixture.path(), &ir)?;
    assert_unchanged("apply_panic_result", fixture.path())?;
    Ok(())
}

#[test]
fn return_lifting_specimen_establishes_both_success_paths() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_lifting")?;
    build(fixture.path())?;
    for (arguments, expected) in [(&[][..], b"11\n".as_slice()), (&["early"][..], b"7\n")] {
        let output = execute(fixture.path(), arguments)?;
        assert!(output.status.success());
        assert_eq!(output.stdout, expected);
    }
    let ir = discover(fixture.path())?;
    assert_functions(&ir, &["value", "main"])?;
    assert_panic_sites(&ir, "value", 0)?;
    let facts = function_facts(&ir, "value")?;
    assert_eq!(facts.signature(), "fn value(early: bool) -> u32");
    assert_eq!(
        facts.declared_output(),
        &DeclaredFunctionOutput::Explicit("u32".into())
    );
    assert_eq!(
        function_facts(&ir, "main")?.declared_output(),
        &DeclaredFunctionOutput::ImplicitUnit
    );
    assert_source_snapshot(fixture.path(), &ir)?;
    assert_unchanged("apply_return_lifting", fixture.path())?;
    Ok(())
}

#[test]
fn unwrap_lookalike_is_a_compiling_negative_specimen() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_unwrap_lookalike")?;
    build(fixture.path())?;
    let output = execute(fixture.path(), &[])?;
    assert!(output.status.success());
    assert_eq!(output.stdout, b"7\n");
    let ir = discover(fixture.path())?;
    assert_functions(&ir, &["leaf", "main"])?;
    // The syntax-based lint flags this too. It is not apply authorization.
    assert_panic_sites(&ir, "leaf", 1)?;
    assert_source_snapshot(fixture.path(), &ir)?;
    assert_unchanged("apply_unwrap_lookalike", fixture.path())?;
    Ok(())
}

#[test]
fn return_evidence_separates_nested_scopes_and_fixed_contracts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_scopes")?;
    build(fixture.path())?;
    for (arguments, expected) in [
        (&[][..], b"10 1 Ok(8)\n".as_slice()),
        (&["early"][..], b"2 1 Ok(8)\n"),
    ] {
        let output = execute(fixture.path(), arguments)?;
        assert!(output.status.success());
        assert_eq!(output.stdout, expected);
    }
    let source = fs::read_to_string(fixture.path().join("src/main.rs")).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let facts = function_facts(&ir, "scoped")?;
    let flow = facts.return_flow();
    assert_eq!(
        flow.returns().len(),
        1,
        "nested returns belong to their own scope"
    );
    let cordial::ReturnValue::Expression(range) = flow.returns()[0].value() else {
        miette::bail!("expected outer return value");
    };
    assert_eq!(&source[range.start()..range.end()], "closure()");
    let cordial::FunctionTail::Expression(range) = flow.tail() else {
        miette::bail!("expected tail expression");
    };
    assert_eq!(&source[range.start()..range.end()], "local() + constant");
    assert_eq!(flow.boundaries(), &[cordial::FunctionBoundary::NestedScope]);
    assert_eq!(
        flow.nested_scopes()
            .iter()
            .map(|scope| scope.kind())
            .collect::<Vec<_>>(),
        [
            cordial::NestedScopeKind::Closure,
            cordial::NestedScopeKind::Item,
            cordial::NestedScopeKind::Async,
            cordial::NestedScopeKind::Const
        ]
    );
    assert_eq!(
        function_facts(&ir, "fixed_contract")?
            .return_flow()
            .boundaries(),
        &[
            cordial::FunctionBoundary::Visible,
            cordial::FunctionBoundary::SpecialSignature
        ]
    );
    assert_eq!(
        function_facts(&ir, "residual")?.return_flow().boundaries(),
        &[cordial::FunctionBoundary::Try]
    );
    let simple = copy_specimen("apply_return_lifting")?;
    let simple_ir = discover(simple.path())?;
    let simple_facts = function_facts(&simple_ir, "value")?;
    let output = simple_facts.output_range();
    let simple_source = fs::read_to_string(simple.path().join("src/main.rs")).into_diagnostic()?;
    assert_eq!(&simple_source[output.start()..output.end()], "u32");
    assert_eq!(simple_facts.return_flow().returns().len(), 1);
    assert!(simple_facts.return_flow().boundaries().is_empty());
    assert_source_snapshot(fixture.path(), &ir)?;
    assert_unchanged("apply_return_scopes", fixture.path())?;
    Ok(())
}

#[test]
fn function_source_facts_distinguish_missing_from_corrupt() -> miette::Result<()> {
    cordial::init_tracing();
    let mut ir = CrateIr::new("missing-or-corrupt");
    assert_eq!(
        ir.function_source_syntax(ir.root()).into_diagnostic()?,
        None
    );
    ir.set_attr(
        ir.root(),
        ATTR_FUNCTION_SOURCE_SYNTAX,
        serde_json::json!({}),
    )
    .into_diagnostic()?;
    assert!(ir.function_source_syntax(ir.root()).is_err());

    let fixture = copy_specimen("apply_return_lifting")?;
    let mut ir = discover(fixture.path())?;
    let function = ir
        .node_by_path("value")
        .ok_or_else(|| miette::miette!("missing value function"))?;
    let mut partial = serde_json::to_value(function_facts(&ir, "value")?).into_diagnostic()?;
    partial
        .as_object_mut()
        .ok_or_else(|| miette::miette!("expected object"))?
        .remove("declared_output");
    ir.set_attr(function, ATTR_FUNCTION_SOURCE_SYNTAX, partial)
        .into_diagnostic()?;
    assert!(
        ir.function_source_syntax(function).is_err(),
        "missing return evidence must not become an implicit unit return"
    );
    Ok(())
}

fn assert_functions(ir: &dyn IrView, expected: &[&str]) -> miette::Result<()> {
    let mut actual: Vec<_> = ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
        .map(|node| {
            node.attr("qualified_path")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .ok_or_else(|| miette::miette!("source function missing its qualified path"))
        })
        .collect::<miette::Result<_>>()?;
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(actual, expected);
    Ok(())
}

fn assert_panic_sites(ir: &dyn IrView, context: &str, count: usize) -> miette::Result<()> {
    let sites: Vec<_> = ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .filter(|node| node.attr("panic_kind").is_some())
        .collect();
    assert_eq!(sites.len(), count);
    let function = ir
        .node_by_path(context)
        .ok_or_else(|| miette::miette!("missing panic-site context"))?;
    for site in sites {
        assert_eq!(
            site.attr("panic_kind").and_then(|value| value.as_str()),
            Some("unwrap")
        );
        assert!(
            ir.parents(site.id(), EdgeKind::Contains)
                .contains(&function)
        );
    }
    Ok(())
}

fn assert_source_snapshot(root: &Path, ir: &dyn IrView) -> miette::Result<()> {
    let source = fs::read(root.join("src/main.rs")).into_diagnostic()?;
    let digest: [u8; 32] = Sha256::digest(&source).into();
    for node in ir.nodes_matching(&BasicQuery::all_nodes()) {
        if node.kind() != &NodeKind::Item(ItemKind::Fn) {
            continue;
        }
        let facts = ir
            .function_source_syntax(node.id())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing source facts"))?;
        assert_eq!(facts.source_digest(), digest);
        let range = facts.signature_range();
        assert_eq!(
            &source[range.start()..range.end()],
            facts.signature().as_bytes()
        );
        let range = facts.body_range();
        assert_eq!(source[range.start()], b'{');
        assert_eq!(source[range.end() - 1], b'}');
    }
    Ok(())
}

fn assert_unchanged(name: &str, root: &Path) -> miette::Result<()> {
    assert_eq!(
        fs::read(root.join("src/main.rs")).into_diagnostic()?,
        fs::read(specimen_path(name)).into_diagnostic()?
    );
    Ok(())
}

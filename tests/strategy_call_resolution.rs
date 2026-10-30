#![cfg(feature = "panics")]
//! Call resolution uses shared declarations and preserves explicit unknowns.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ATTR_FUNCTION_CALL_FLOW, CallResolutionFailure, EdgeKind, IrView, SourceCallResolution,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::fs;

#[test]
fn direct_calls_retain_identity_ranges_and_incoming_edges() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_call_resolution.rs"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let node = |name| {
        ir.node_by_path(name)
            .ok_or_else(|| miette::miette!("missing {name}"))
    };
    for (caller, target, count) in [
        ("leaf", "source", 1),
        ("caller", "leaf", 2),
        ("recurse", "recurse", 1),
    ] {
        let flow = ir
            .function_call_flow(node(caller)?)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing flow"))?;
        assert!(flow.namespace_trusted());
        assert_eq!(flow.calls().len(), count);
        let target_id = node(target)?;
        for call in flow.calls() {
            assert_eq!(
                call.resolution(),
                &SourceCallResolution::Source {
                    function: target_id,
                    source_digest: function_facts(&ir, target)?.source_digest()
                }
            );
        }
        assert!(
            ir.parents(target_id, EdgeKind::Calls)
                .contains(&node(caller)?)
        );
        assert_eq!(ir.children(node(caller)?, EdgeKind::Calls), [target_id]);
    }
    for name in ["shadow", "local", "external", "qualified", "calls_generic"] {
        let flow = ir
            .function_call_flow(node(name)?)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing flow"))?;
        assert!(
            flow.calls()
                .iter()
                .all(|call| matches!(call.resolution(), SourceCallResolution::Unresolved(_)))
        );
        assert!(ir.children(node(name)?, EdgeKind::Calls).is_empty());
    }
    assert!(function_facts(&ir, "scopes")?.unwraps().is_empty());
    Ok(())
}

#[test]
fn imports_attributes_macros_and_extern_aliases_keep_names_unresolved() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"Ok(8)\n");
    let (doc, body) = original
        .split_once('\n')
        .ok_or_else(|| miette::miette!("missing specimen header"))?;
    for prefix in [
        "use std::io;\n",
        "extern crate std as platform;\n",
        "#![no_implicit_prelude]\n",
        "macro_rules! opaque { () => {} }\n",
    ] {
        fs::write(&path, format!("{doc}\n{prefix}{body}")).into_diagnostic()?;
        let ir = discover(fixture.path())?;
        let leaf = ir
            .node_by_path("leaf")
            .ok_or_else(|| miette::miette!("missing leaf"))?;
        let flow = ir
            .function_call_flow(leaf)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing flow"))?;
        assert!(!flow.namespace_trusted());
        assert!(flow.calls().iter().all(|call| call.resolution()
            == &SourceCallResolution::Unresolved(CallResolutionFailure::OpaqueScope)));
        assert!(ir.children(leaf, EdgeKind::Calls).is_empty());
    }
    Ok(())
}

#[test]
fn root_namespace_ambiguity_reaches_other_source_modules() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let source = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&source).into_diagnostic()?;
    fs::write(fixture.path().join("src/remote.rs"), original).into_diagnostic()?;
    fs::write(
        &source,
        "extern crate std as platform;\nmod remote;\nfn main() {}\n",
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("remote::leaf")
        .ok_or_else(|| miette::miette!("missing remote leaf"))?;
    assert!(
        !ir.function_call_flow(leaf)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing flow"))?
            .namespace_trusted()
    );
    assert!(ir.children(leaf, EdgeKind::Calls).is_empty());
    Ok(())
}

#[test]
fn missing_and_corrupt_call_evidence_are_distinct() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let mut ir = discover(fixture.path())?;
    assert!(
        ir.function_call_flow(ir.root())
            .into_diagnostic()?
            .is_none()
    );
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    ir.set_attr(leaf, ATTR_FUNCTION_CALL_FLOW, serde_json::json!({}))
        .into_diagnostic()?;
    assert!(ir.function_call_flow(leaf).is_err());
    Ok(())
}

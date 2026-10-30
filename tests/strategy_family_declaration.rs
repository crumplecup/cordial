#![cfg(feature = "panics")]
//! Executable proof of supported nested source wrappers and conversions.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ATTR_SOURCE_MODULE_SCOPE, EdgeKind, ErrorContractBroadening as Broadening,
    ErrorDeclarationBlocker, ErrorFamilyBasis, ErrorFamilyDeclarationBlocker as Blocker,
    ErrorFamilyDeclarationDecision as Decision, ErrorFamilyReusePolicy, IrView, NodeId,
    OwnedErrorFamilyDeclaration, SourceErrorReuse, plan_owned_error_family_declaration,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::{fs, num::NonZeroUsize};

fn fixture() -> miette::Result<(tempfile::TempDir, cordial::CrateIr)> {
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_family_representation.rs.txt"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    Ok((fixture, ir))
}

fn scope(ir: &dyn IrView, names: &[&str]) -> miette::Result<Vec<NodeId>> {
    names
        .iter()
        .map(|name| {
            ir.node_by_path(name)
                .ok_or_else(|| miette::miette!("missing {name}"))
        })
        .collect()
}

fn policy(reuse: SourceErrorReuse) -> miette::Result<ErrorFamilyReusePolicy> {
    Ok(ErrorFamilyReusePolicy::new(
        ErrorFamilyBasis::PanicMigration,
        NonZeroUsize::new(3).ok_or_else(|| miette::miette!("threshold"))?,
        reuse,
    ))
}

fn ready(decision: Decision) -> miette::Result<OwnedErrorFamilyDeclaration> {
    let Decision::Ready(declaration) = decision else {
        return Err(miette::miette!("unexpected refusal: {decision:?}"));
    };
    Ok(declaration)
}

#[test]
fn four_level_owned_families_compile_and_preserve_all_native_sources() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, ir) = fixture()?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    let functions = scope(&ir, &["single", "a", "nested"])?;
    let root = scope(&ir, &["all"])?[0];
    let declaration = ready(
        plan_owned_error_family_declaration(
            &ir,
            &functions,
            root,
            policy(SourceErrorReuse::Forbidden)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
    )?;
    assert_eq!(declaration.families().len(), 4);
    assert_eq!(declaration.root(), 3);
    for (index, family) in declaration.families().iter().enumerate() {
        assert_eq!(family.family(), index);
        assert_eq!(family.sources().len(), 1);
        assert_eq!(
            family.sources()[0].member(),
            &declaration.layout().families()[index].direct_members()[0]
        );
    }
    assert_eq!(declaration.plan().function(), root);
    assert_eq!(declaration.plan().path(), &path);
    assert_eq!(
        declaration.plan().source_digest(),
        function_facts(&ir, "all")?.source_digest()
    );
    let preview = declaration.plan().preview(&original).into_diagnostic()?;
    assert!(preview.starts_with(&original));
    assert!(
        declaration
            .plan()
            .preview(&format!("{original}\n"))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    let mut reordered = functions.clone();
    reordered.reverse();
    reordered.push(root);
    reordered.push(root);
    assert_eq!(
        plan_owned_error_family_declaration(
            &ir,
            &reordered,
            root,
            policy(SourceErrorReuse::Forbidden)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
        Decision::Ready(declaration.clone())
    );
    let families = declaration.families();
    let consumer = include_str!("fixtures/strategy_nested_family_consumer.rs.txt")
        .replace("$ROOT", families[3].wrapper())
        .replace("$TRIPLE", families[2].wrapper())
        .replace("$PAIR", families[1].wrapper())
        .replace("$IO_FAMILY", families[0].wrapper())
        .replace("$IO_SOURCE", families[0].sources()[0].wrapper())
        .replace("$PARSE_SOURCE", families[1].sources()[0].wrapper())
        .replace("$UTF8_SOURCE", families[2].sources()[0].wrapper())
        .replace("$FLOAT_SOURCE", families[3].sources()[0].wrapper());
    fs::write(
        &path,
        format!(
            "{}\n{consumer}",
            preview.replace("fn main()", "fn baseline_main()")
        ),
    )
    .into_diagnostic()?;
    build(fixture.path())?;
    let output = execute(fixture.path(), &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"nested sources, contexts and native payloads preserved\n"
    );
    assert!(output.stderr.is_empty());
    assert_compliance(fixture.path())?;
    // A fresh production collection cannot authorize duplicate declarations.
    let fresh = discover(fixture.path())?;
    let functions = scope(&fresh, &["single", "a", "nested"])?;
    assert!(matches!(
        plan_owned_error_family_declaration(
            &fresh,
            &functions,
            scope(&fresh, &["all"])?[0],
            policy(SourceErrorReuse::Forbidden)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
        Decision::Refused(_)
    ));
    Ok(())
}

#[cfg(feature = "internal_error_chain")]
fn assert_compliance(root: &std::path::Path) -> miette::Result<()> {
    let report =
        cordial::scan_crate_internal_error_chain(root, "strategy-specimen").into_diagnostic()?;
    assert!(
        report.compliance().findings().is_empty(),
        "{:?}",
        report.compliance().findings()
    );
    Ok(())
}

#[cfg(not(feature = "internal_error_chain"))]
fn assert_compliance(_root: &std::path::Path) -> miette::Result<()> {
    Ok(())
}

#[test]
fn only_selected_root_children_are_emitted_under_permitted_reuse() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, ir) = fixture()?;
    let path = fixture.path().join("src/main.rs");
    let source = fs::read_to_string(&path).into_diagnostic()?;
    let functions = scope(&ir, &["a", "a_again", "a_third", "b", "b_again", "b_third"])?;
    let declaration = ready(
        plan_owned_error_family_declaration(
            &ir,
            &functions,
            scope(&ir, &["nested"])?[0],
            policy(SourceErrorReuse::Permitted)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
    )?;
    assert_eq!(declaration.layout().families().len(), 3);
    assert_eq!(declaration.families().len(), 2);
    let root = &declaration.families()[1];
    assert_eq!(root.family(), declaration.root());
    assert_eq!(root.children().len(), 1);
    assert_eq!(root.sources().len(), 1);
    assert_eq!(
        root.sources()[0].member().identity(),
        "::std::str::Utf8Error"
    );
    let child = &declaration.families()[0];
    assert_eq!(child.sources().len(), 2);
    let preview = declaration.plan().preview(&source).into_diagnostic()?;
    let consumer = format!(
        "fn main() {{ baseline_main(); let error = {}::from(::std::io::Error::from(::std::io::ErrorKind::NotFound)); let child = ::std::error::Error::source(&error).unwrap(); assert!(child.is::<{}>()); }}",
        root.wrapper(),
        child.wrapper(),
    );
    fs::write(
        &path,
        format!(
            "{}\n{consumer}",
            preview.replace("fn main()", "fn baseline_main()")
        ),
    )
    .into_diagnostic()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    assert_compliance(fixture.path())?;
    Ok(())
}

#[test]
fn all_generated_names_are_reserved_and_module_facts_are_checked() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, mut ir) = fixture()?;
    let functions = scope(&ir, &["single", "a", "nested"])?;
    let root = scope(&ir, &["all"])?[0];
    let reuse = policy(SourceErrorReuse::Forbidden)?;
    let declaration = ready(
        plan_owned_error_family_declaration(&ir, &functions, root, reuse, Broadening::Forbidden)
            .into_diagnostic()?,
    )?;
    let module = ir.parents(root, EdgeKind::Contains)[0];
    let original = serde_json::to_value(ir.source_module_scope(module).into_diagnostic()?)
        .into_diagnostic()?;
    for family in declaration.families() {
        for name in [family.wrapper(), family.kind()]
            .into_iter()
            .chain(family.sources().iter().map(|source| source.wrapper()))
        {
            let mut bad = original.clone();
            bad["identifiers"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("identifiers"))?
                .push(serde_json::json!(name));
            ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, bad)
                .into_diagnostic()?;
            assert_eq!(
                plan_owned_error_family_declaration(
                    &ir,
                    &functions,
                    root,
                    reuse,
                    Broadening::Forbidden
                )
                .into_diagnostic()?,
                Decision::Refused(Blocker::Declaration(
                    ErrorDeclarationBlocker::NameCollision(name.clone())
                ))
            );
        }
    }
    for (key, value) in [
        ("insertion_offset", serde_json::json!(0)),
        ("source_digest", serde_json::json!(vec![0; 32])),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, bad)
            .into_diagnostic()?;
        assert_eq!(
            plan_owned_error_family_declaration(
                &ir,
                &functions,
                root,
                reuse,
                Broadening::Forbidden
            )
            .into_diagnostic()?,
            Decision::Refused(Blocker::Declaration(
                ErrorDeclarationBlocker::InvalidModuleEvidence
            ))
        );
    }
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, serde_json::json!({}))
        .into_diagnostic()?;
    assert!(
        plan_owned_error_family_declaration(&ir, &functions, root, reuse, Broadening::Forbidden)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("src/main.rs")).into_diagnostic()?,
        include_str!("fixtures/strategy_family_representation.rs.txt")
    );
    Ok(())
}

#[test]
fn incomplete_and_infallible_scopes_do_not_expose_partial_edits() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let reuse = policy(SourceErrorReuse::Forbidden)?;
    for names in [&["a", "b"][..], &["unknown"][..]] {
        let functions = scope(&ir, names)?;
        assert!(matches!(
            plan_owned_error_family_declaration(
                &ir,
                &functions,
                functions[0],
                reuse,
                Broadening::Forbidden
            )
            .into_diagnostic()?,
            Decision::Refused(Blocker::Representation(_))
        ));
    }
    let empty = scope(&ir, &["empty"])?[0];
    assert_eq!(
        plan_owned_error_family_declaration(&ir, &[], empty, reuse, Broadening::Forbidden)
            .into_diagnostic()?,
        Decision::Infallible
    );
    assert!(matches!(
        plan_owned_error_family_declaration(&ir, &[], ir.root(), reuse, Broadening::Forbidden)
            .into_diagnostic()?,
        Decision::Refused(Blocker::Representation(_))
    ));
    Ok(())
}

#[test]
fn unsupported_nominal_errors_and_multiple_modules_are_explicit_refusals() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, _) = fixture()?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    fs::write(
        &path,
        original.replace("::std::io::Error", "::std::string::String"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let root = scope(&ir, &["single"])?[0];
    assert_eq!(
        plan_owned_error_family_declaration(
            &ir,
            &[],
            root,
            policy(SourceErrorReuse::Permitted)?,
            Broadening::Forbidden
        )
        .into_diagnostic()?,
        Decision::Refused(Blocker::UnsupportedSourceType(
            "::std::string::String".to_owned()
        ))
    );
    fs::write(
        &path,
        format!(
            "mod other;\n{}",
            original.replace("let _ = (", "let _ = (other::empty(),")
        ),
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/other.rs"),
        "pub fn empty() -> u32 { 0 }\n",
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    assert_eq!(
        plan_owned_error_family_declaration(
            &ir,
            &scope(&ir, &["other::empty"])?,
            scope(&ir, &["single"])?[0],
            policy(SourceErrorReuse::Permitted)?,
            Broadening::Forbidden
        )
        .into_diagnostic()?,
        Decision::Refused(Blocker::Declaration(
            ErrorDeclarationBlocker::InvalidModuleEvidence
        ))
    );
    Ok(())
}

use std::fs;
use std::path::{Path, PathBuf};

use cordial::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, RunAll, Session, SessionBuilder,
    scan_antipatterns_rust_source, scan_crate_antipatterns,
};

use miette::{IntoDiagnostic, WrapErr};

fn fixture(name: &str) -> miette::Result<String> {
    fs::read_to_string(format!(
        "{}/tests/fixtures/quality/antipatterns/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .into_diagnostic()
    .wrap_err_with(|| format!("read fixture {name}"))
}

fn scan_fixture(name: &str) -> miette::Result<Vec<cordial::AntipatternSiteRecord>> {
    let src_root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/quality/antipatterns"
    ));
    let file = src_root.join(name);
    scan_antipatterns_rust_source(&fixture(name)?, &file, src_root, src_root).into_diagnostic()
}

#[test]
fn box_dyn_error_sites_are_detected() -> miette::Result<()> {
    let findings = scan_fixture("box_dyn_error.rs")?;
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule_id == AntipatternRuleId::BoxDynError001)
            .count(),
        4
    );
    Ok(())
}

#[test]
fn box_dyn_error_context_includes_enclosing_fn() -> miette::Result<()> {
    let findings = scan_fixture("box_dyn_error.rs")?;
    let accepts = findings
        .iter()
        .find(|f| f.context.contains("accepts"))
        .ok_or_else(|| miette::miette!("accepts finding"))?;
    assert_eq!(accepts.rule_id, AntipatternRuleId::BoxDynError001);
    assert!(accepts.snippet.contains("Box<dyn Error>"));
    Ok(())
}

#[test]
fn non_box_dyn_error_types_are_ignored() -> miette::Result<()> {
    let findings = scan_fixture("box_dyn_error.rs")?;
    assert!(
        !findings
            .iter()
            .any(|f| f.snippet.contains("Display") || f.snippet.contains("&'static"))
    );
    Ok(())
}

#[test]
fn string_error_result_types_are_detected() -> miette::Result<()> {
    let findings = scan_fixture("string_error.rs")?;
    let string_errors: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id == AntipatternRuleId::StringError001)
        .collect();
    assert_eq!(string_errors.len(), 4, "{string_errors:?}");
    assert!(
        string_errors
            .iter()
            .any(|f| f.context.contains("returns_string"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context.contains("returns_str"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context.contains("returns_std_string"))
    );
    assert!(
        string_errors
            .iter()
            .any(|f| f.context.contains("StringResult"))
    );
    assert!(
        !string_errors
            .iter()
            .any(|f| f.context.contains("ok_is_string") || f.context.contains("typed_error"))
    );
    Ok(())
}

#[test]
fn unused_underscore_arguments_are_detected() -> miette::Result<()> {
    let findings = scan_fixture("unused_underscore_args.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();
    assert_eq!(unused.len(), 6);
    assert!(
        unused
            .iter()
            .any(|f| f.context.contains("Bot") && f.context.contains("handle"))
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context.contains("free_fn") && f.snippet == "_x")
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context.contains("tuple") && f.snippet == "_c")
    );
    assert!(
        !unused
            .iter()
            .any(|f| f.context.contains("test_fn") || f.snippet == "y" || f.snippet == "b")
    );
    Ok(())
}

#[test]
fn foreign_trait_impl_unused_args_are_skipped() -> miette::Result<()> {
    let findings = scan_fixture("foreign_trait_unused_args.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();
    assert_eq!(unused.len(), 2, "{unused:?}");
    assert!(
        unused
            .iter()
            .any(|f| f.context.contains("Mine") && f.snippet == "_arg")
    );
    assert!(unused.iter().any(|f| f.snippet == "_z"));
    assert!(
        !unused
            .iter()
            .any(|f| f.context.contains("visit_expr_closure") || f.snippet == "_node")
    );
    Ok(())
}

#[test]
fn crate_local_traits_apply_across_files() -> miette::Result<()> {
    let tmp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let src = tmp.path().join("src");
    fs::create_dir_all(&src)
        .into_diagnostic()
        .wrap_err("mkdir src")?;
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"split_traits\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    fs::write(
        src.join("lib.rs"),
        "mod query;\n\
         struct Hub;\n\
         impl query::Query for Hub {\n\
             fn matches_node(&self, _node: u8) {}\n\
         }\n\
         struct Walker;\n\
         impl Visit for Walker {\n\
             fn visit_expr_closure(&mut self, _node: u8) {}\n\
         }\n",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        src.join("query.rs"),
        "pub trait Query { fn matches_node(&self, node: u8); }\n",
    )
    .into_diagnostic()
    .wrap_err("write query")?;

    let findings = scan_crate_antipatterns(tmp.path(), "split_traits", tmp.path(), tmp.path())
        .into_diagnostic()
        .wrap_err("scan crate")?;
    let unused: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect();
    assert!(
        unused
            .iter()
            .any(|f| f.snippet == "_node" && f.context.contains("matches_node")),
        "{unused:?}"
    );
    assert!(
        !unused
            .iter()
            .any(|f| f.context.contains("visit_expr_closure")),
        "{unused:?}"
    );
    Ok(())
}

#[test]
fn trait_declarations_do_not_flag_placeholder_params() -> miette::Result<()> {
    let findings = scan_fixture("unused_underscore_args.rs")?;
    assert!(!findings.iter().any(|f| {
        f.rule_id == AntipatternRuleId::UnusedUnderscoreArg001
            && f.context.contains("Declared")
            && f.context.contains("placeholder")
    }));
    Ok(())
}

#[test]
fn static_struct_fields_are_detected() -> miette::Result<()> {
    let findings = scan_fixture("static_struct_fields.rs")?;
    let static_refs = findings
        .iter()
        .filter(|f| f.rule_id == AntipatternRuleId::StructStaticRef001)
        .collect::<Vec<_>>();
    assert_eq!(static_refs.len(), 8);
    assert!(
        static_refs
            .iter()
            .any(|f| f.context.contains("BorrowsStatic") && f.context.ends_with("::name"))
    );
    assert!(
        static_refs
            .iter()
            .any(|f| f.context.contains("Message::Inline") && f.context.ends_with("::_0"))
    );
    assert!(
        !static_refs
            .iter()
            .any(|f| f.context.contains("OwnsData") || f.context.contains("Named::detail"))
    );
    Ok(())
}

#[test]
fn enum_variant_tuple_and_struct_payloads_are_detected() -> miette::Result<()> {
    let findings = scan_fixture("static_struct_fields.rs")?;
    let variant_findings: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.rule_id == AntipatternRuleId::StructStaticRef001
                && (f.context.contains("Message::") || f.context.contains("Payload::"))
        })
        .collect();
    assert_eq!(variant_findings.len(), 5);
    Ok(())
}

#[test]
fn fn_signatures_with_static_refs_are_not_struct_fields() -> miette::Result<()> {
    let findings = scan_fixture("static_struct_fields.rs")?;
    assert!(!findings.iter().any(|f| {
        f.rule_id == AntipatternRuleId::StructStaticRef001
            && (f.context.contains("accepts") || f.context.contains("returns"))
    }));
    Ok(())
}

#[test]
fn antipatterns_etiquette_emits_reports() -> miette::Result<()> {
    let workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/box_dyn_error");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register(&ANTIPATTERNS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(
        outcome
            .findings()
            .filter(|f| f.rule().category() == "antipatterns")
            .count(),
        4
    );

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("antipatterns.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("ANTIPATTERN-BOX-DYN-ERROR-001"));

    let checklist = fs::read_to_string(findings_dir.join("antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("accepts"));

    let summary = fs::read_to_string(findings_dir.join("antipatterns-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("box_dyn_error **4**"));
    Ok(())
}

#[test]
fn aliased_one_arg_result_is_not_a_string_error() -> miette::Result<()> {
    let src_root = Path::new(".");
    let file = src_root.join("aliased.rs");
    let findings = scan_antipatterns_rust_source(
        "fn io_ok() -> std::io::Result<u8> { Ok(0) }\n\
         fn report() -> miette::Result<String> { Ok(String::new()) }\n",
        &file,
        src_root,
        src_root,
    )
    .into_diagnostic()?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule_id != AntipatternRuleId::StringError001),
        "one-arg Result aliases are not Result<_, String>: {findings:?}"
    );
    Ok(())
}

#[test]
fn documented_exceptions_suppress_checklist_items() -> miette::Result<()> {
    let workspace = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(workspace.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        workspace.path().join("src/lib.rs"),
        fixture("unused_underscore_args.rs")?,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;
    fs::write(
        workspace.path().join("Cargo.toml"),
        r#"[workspace]
members = ["."]

[workspace.package]
version = "0.1.0"
edition = "2024"

[package]
name = "fixture"
version = { workspace = true }
edition = { workspace = true }
"#,
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    fs::create_dir_all(store.path().join("exceptions/antipatterns"))
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        store.path().join("exceptions/antipatterns/fixture.json"),
        r#"[
  {
    "file": "src/lib.rs",
    "context": "Bot::handle",
    "reason": "fixture exception"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception")?;

    let session = SessionBuilder::new(workspace.path())
        .with_store_root(store.path())
        .register(&ANTIPATTERNS_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let open = outcome
        .findings()
        .filter(|f| {
            f.rule().category() == "antipatterns" && f.disposition() == cordial::Disposition::Open
        })
        .count();
    assert_eq!(open, 5);

    let checklist = fs::read_to_string(store.path().join("findings/antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("Documented exceptions"));
    assert!(checklist.contains("fixture exception"));
    Ok(())
}

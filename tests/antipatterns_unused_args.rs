//! Unused arguments: which are flagged and which are skipped (foreign traits, proc macros, cfg siblings).

use cordial::AntipatternRuleId;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/antipatterns_crate.rs"]
mod antipatterns_crate;

#[path = "support/antipatterns_fixture.rs"]
mod antipatterns_fixture;

#[path = "support/antipatterns_scan.rs"]
mod antipatterns_scan;

use antipatterns_crate::crate_antipatterns;
use antipatterns_scan::scan_fixture;

#[test]
fn unused_underscore_arguments_are_detected() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("unused_underscore_args.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();
    assert_eq!(unused.len(), 5);
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("Bot") && f.context().contains("handle"))
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("free_fn") && f.snippet() == "_x")
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("tuple") && f.snippet() == "_c")
    );
    assert!(!unused.iter().any(|f| f.context().contains("test_fn")
        || f.context().contains("ignored")
        || f.snippet() == "y"
        || f.snippet() == "b"));
    Ok(())
}

#[test]
fn foreign_trait_impl_unused_args_are_skipped() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("foreign_trait_unused_args.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();
    assert_eq!(unused.len(), 2, "{unused:?}");
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("Mine") && f.snippet() == "_arg")
    );
    assert!(unused.iter().any(|f| f.snippet() == "_z"));
    assert!(
        !unused
            .iter()
            .any(|f| f.context().contains("visit_expr_closure") || f.snippet() == "_node")
    );
    Ok(())
}

#[test]
fn proc_macro_abi_and_creusot_opaque_stub_unused_args_are_skipped() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("unused_underscore_args_exempt_signatures.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();

    // Exempt: proc-macro ABI functions and the real #[logic(opaque)] +
    // `dead`-body Creusot idiom.
    assert!(
        !unused.iter().any(|f| f.context().contains("my_attr")),
        "{unused:?}"
    );
    assert!(
        !unused.iter().any(|f| f.context().contains("my_derive")),
        "{unused:?}"
    );
    assert!(
        !unused.iter().any(|f| f.context().contains("my_macro")),
        "{unused:?}"
    );
    assert!(
        !unused
            .iter()
            .any(|f| f.context().contains("opaque_len") && !f.context().contains("real_body")),
        "{unused:?}"
    );

    // Not exempt: #[logic(opaque)] without a bare `dead` body, and a
    // bare `dead` body without #[logic(opaque)] -- both still flagged,
    // proving the check requires both signals together, not either alone.
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("opaque_len_with_real_body")),
        "{unused:?}"
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("looks_like_dead_but_is_not")),
        "{unused:?}"
    );
    Ok(())
}

#[test]
fn cfg_sibling_unused_args_are_skipped_when_a_sibling_reads_them() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("unused_underscore_args_cfg_siblings.rs")?;
    let unused = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect::<Vec<_>>();

    // Exempt: a cfg sibling (free fn and impl method) genuinely reads
    // the same-named parameter.
    assert!(
        !unused
            .iter()
            .any(|f| f.context().contains("decide") && f.snippet() == "_bytes"),
        "{unused:?}"
    );
    assert!(
        !unused
            .iter()
            .any(|f| f.context().contains("read") && f.snippet() == "_source"),
        "{unused:?}"
    );

    // Not exempt: neither cfg sibling reads `extra`, and `solo` has no
    // same-named sibling at all -- both still flagged.
    assert!(
        unused
            .iter()
            .filter(|f| f.context().contains("unrelated") && f.snippet() == "_extra")
            .count()
            == 2,
        "{unused:?}"
    );
    assert!(
        unused
            .iter()
            .any(|f| f.context().contains("solo") && f.snippet() == "_alone"),
        "{unused:?}"
    );
    Ok(())
}

#[test]
fn crate_local_traits_apply_across_files() -> miette::Result<()> {
    cordial::init_tracing();
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

    let findings =
        crate_antipatterns(tmp.path(), "split_traits", tmp.path()).wrap_err("scan crate")?;
    let unused: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001)
        .collect();
    assert!(
        unused
            .iter()
            .any(|f| f.snippet() == "_node" && f.context().contains("matches_node")),
        "{unused:?}"
    );
    assert!(
        !unused
            .iter()
            .any(|f| f.context().contains("visit_expr_closure")),
        "{unused:?}"
    );
    Ok(())
}

#[test]
fn trait_declarations_do_not_flag_placeholder_params() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_fixture("unused_underscore_args.rs")?;
    assert!(!findings.iter().any(|f| {
        f.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001
            && f.context().contains("Declared")
            && f.context().contains("placeholder")
    }));
    Ok(())
}

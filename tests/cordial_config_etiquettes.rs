//! Each etiquette's own table in `cordial.toml`: strategies, thresholds, and drift gates.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    DependencyFreshnessThresholds, ErrorFieldStrategy, StaticRefStrategy, load_cordial_config,
};

#[test]
fn error_field_strategy_defaults_to_owned_and_accepts_borrowed() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic()?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    assert_eq!(
        load_cordial_config(workspace.path(), store_home.path())
            .internal_error_chain()
            .field_strategy(),
        ErrorFieldStrategy::Owned
    );
    fs::write(
        workspace.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    assert_eq!(
        load_cordial_config(workspace.path(), store_home.path())
            .internal_error_chain()
            .field_strategy(),
        ErrorFieldStrategy::Borrowed
    );
    Ok(())
}

#[test]
fn kind_fanout_defaults_to_zero_and_accepts_override() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic()?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    assert_eq!(
        load_cordial_config(workspace.path(), store_home.path())
            .internal_error_chain()
            .kind_fanout_max_call_sites(),
        0
    );
    fs::write(
        workspace.path().join("cordial.toml"),
        "[internal_error_chain]\nkind_fanout_max_call_sites = 2\n",
    )
    .into_diagnostic()?;
    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(
        loaded.internal_error_chain().kind_fanout_max_call_sites(),
        2
    );
    assert!(loaded.etiquette_enabled("internal_error_chain"));
    Ok(())
}

#[test]
fn derives_toml_overrides_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[derives]
max_constructor_args = 5
min_fluent_setters = 3
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(loaded.derives().max_constructor_args(), 5);
    assert_eq!(loaded.derives().min_fluent_setters(), 3);
    Ok(())
}

#[test]
fn crate_attrs_toml_overrides_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[crate_attrs]
forbid_unsafe = false
missing_docs = true
allow_unsafe = ["ffi"]
allow_missing_docs = ["legacy"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let crate_attrs = loaded.crate_attrs();
    assert!(!crate_attrs.forbid_unsafe());
    assert!(crate_attrs.missing_docs());
    assert_eq!(crate_attrs.allow_unsafe(), &["ffi".to_string()]);
    assert_eq!(crate_attrs.allow_missing_docs(), &["legacy".to_string()]);
    Ok(())
}

#[test]
fn doc_warnings_toml_overrides_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[doc_warnings]
document_private_items = true
all_features = true
skip_crates = ["proc_helper"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let doc_warnings = loaded.doc_warnings();
    assert!(doc_warnings.document_private_items());
    assert!(doc_warnings.all_features());
    assert_eq!(doc_warnings.skip_crates(), &["proc_helper".to_string()]);
    assert!(doc_warnings.enabled());
    Ok(())
}

#[test]
fn creusot_diagnostics_toml_overrides_default() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[creusot_diagnostics]
skip_crates = ["slow_proof"]
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    let creusot_diagnostics = loaded.creusot_diagnostics();
    assert_eq!(
        creusot_diagnostics.skip_crates(),
        &["slow_proof".to_string()]
    );
    assert!(creusot_diagnostics.enabled());
    Ok(())
}

#[test]
fn antipatterns_static_ref_strategy_defaults_to_string() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(
        loaded.antipatterns().static_refs().strategy(),
        StaticRefStrategy::String
    );
    assert!(loaded.etiquette_enabled("antipatterns"));
    Ok(())
}

#[test]
fn antipatterns_static_ref_strategy_reads_cordial_toml() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[antipatterns.static_refs]
strategy = "cow"
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_eq!(
        loaded.antipatterns().static_refs().strategy(),
        StaticRefStrategy::Cow
    );
    assert!(loaded.etiquette_enabled("antipatterns"));
    Ok(())
}

#[test]
fn antipatterns_enabled_false_still_works_with_strategy_policy() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[antipatterns]
enabled = false

[antipatterns.static_refs]
strategy = "const"
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert!(!loaded.etiquette_enabled("antipatterns"));
    assert_eq!(
        loaded.antipatterns().static_refs().strategy(),
        StaticRefStrategy::Const
    );
    Ok(())
}

#[test]
fn dependency_freshness_policy_reads_drift_class_gates() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        r#"
[dependency_freshness]
patch = false
minor = true
major = false
manifest_exact_pin = false
manifest_upper_bound = true
manifest_wildcard = false
manifest_tilde = true
manifest_workspace_bypass = false
"#,
    )
    .into_diagnostic()
    .wrap_err("workspace config")?;

    let loaded = load_cordial_config(workspace.path(), store_home.path());
    assert_ne!(
        loaded.dependency_freshness(),
        &DependencyFreshnessThresholds::default()
    );
    assert!(!loaded.dependency_freshness().patch());
    assert!(loaded.dependency_freshness().minor());
    assert!(!loaded.dependency_freshness().major());
    assert!(!loaded.dependency_freshness().manifest_exact_pin());
    assert!(loaded.dependency_freshness().manifest_upper_bound());
    assert!(!loaded.dependency_freshness().manifest_wildcard());
    assert!(loaded.dependency_freshness().manifest_tilde());
    assert!(!loaded.dependency_freshness().manifest_workspace_bypass());
    assert!(
        !loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-PATCH")
    );
    assert!(
        loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MINOR")
    );
    assert!(
        !loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MAJOR")
    );
    assert!(
        !loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN")
    );
    assert!(
        loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND")
    );
    assert!(
        !loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD")
    );
    assert!(
        loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MANIFEST-TILDE")
    );
    assert!(
        !loaded
            .dependency_freshness()
            .rule_enabled("DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS")
    );
    Ok(())
}

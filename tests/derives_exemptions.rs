//! What the derives lint leaves alone: `const fn`, path-spliced files, clap schemas, and cfg-gated structs.

use cordial::{DeriveRuleId, DerivesThresholds, scan_derives_rust_source};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/derives_scan.rs"]
mod derives_scan;

#[path = "support/derives_rules.rs"]
mod derives_rules;

use derives_rules::scan_rules;
use derives_scan::scan_findings;

#[test]
fn const_fn_new_getter_setter_and_as_ref_are_all_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    name: String,
    count: u32,
}

impl Widget {
    pub const fn new(name: String, count: u32) -> Self {
        Self { name, count }
    }

    pub const fn count(&self) -> u32 {
        self.count
    }

    pub const fn set_count(&mut self, count: u32) {
        self.count = count;
    }

    pub const fn as_ref(&self) -> &str {
        self.name.as_ref()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "const fn constructor/getter/setter/as_ref have no const-preserving \
         derive to recommend: {findings:?}"
    );
    Ok(())
}

#[test]
fn non_const_counterpart_still_flags_all_four() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    name: String,
    count: u32,
}

impl Widget {
    pub fn new(name: String, count: u32) -> Self {
        Self { name, count }
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn set_count(&mut self, count: u32) {
        self.count = count;
    }

    pub fn as_ref(&self) -> &str {
        self.name.as_ref()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    for expected in [
        DeriveRuleId::New001,
        DeriveRuleId::Getter001,
        DeriveRuleId::Setter001,
        DeriveRuleId::AsRef001,
    ] {
        assert!(
            findings.contains(&expected),
            "dropping `const` should be the only difference from the exempt \
             fixture, so the non-const version must still flag {expected:?}: {findings:?}"
        );
    }
    Ok(())
}

/// Real two-crate workspace: `owner` defines a struct with a manual
/// getter; `consumer` splices `owner`'s file in via `#[path]` (the real
/// `amenable_verus`/`amenable_core` pattern this whole mechanism exists
/// for). Returns the workspace root and `owner`'s crate root/src root/
/// file path, so a test can run the real `scan_derives_rust_source` scan
/// against them.
fn write_path_spliced_fixture(
    consumer_has_getters_dep: bool,
) -> miette::Result<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let root = fixture.path();

    fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
members = ["crates/owner", "crates/consumer", "fake_deps/derive_getters"]
resolver = "2"
"#,
    )
    .into_diagnostic()?;

    fs::create_dir_all(root.join("fake_deps/derive_getters/src")).into_diagnostic()?;
    fs::write(
        root.join("fake_deps/derive_getters/Cargo.toml"),
        r#"
[package]
name = "derive_getters"
version = "0.1.0"
edition = "2024"
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.join("fake_deps/derive_getters/src/lib.rs"),
        "// stand-in for the real derive_getters crate",
    )
    .into_diagnostic()?;

    let owner_root = root.join("crates/owner");
    fs::create_dir_all(owner_root.join("src")).into_diagnostic()?;
    fs::write(
        owner_root.join("Cargo.toml"),
        r#"
[package]
name = "owner"
version = "0.1.0"
edition = "2024"
"#,
    )
    .into_diagnostic()?;
    let owner_file = owner_root.join("src/shared.rs");
    fs::write(
        &owner_file,
        r#"pub struct Widget {
    name: String,
}

impl Widget {
    pub fn name(&self) -> &str {
        &self.name
    }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(owner_root.join("src/lib.rs"), "mod shared;\n").into_diagnostic()?;

    let consumer_root = root.join("crates/consumer");
    fs::create_dir_all(consumer_root.join("src")).into_diagnostic()?;
    let consumer_dep = if consumer_has_getters_dep {
        r#"derive_getters = { path = "../../fake_deps/derive_getters" }"#
    } else {
        ""
    };
    fs::write(
        consumer_root.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "consumer"
version = "0.1.0"
edition = "2024"

[dependencies]
{consumer_dep}
"#
        ),
    )
    .into_diagnostic()?;
    fs::write(
        consumer_root.join("src/lib.rs"),
        r#"#[path = "../../owner/src/shared.rs"]
mod shared;
"#,
    )
    .into_diagnostic()?;

    Ok((fixture, owner_root, owner_file))
}

#[test]
fn path_spliced_file_without_the_dependency_is_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, owner_root, owner_file) = write_path_spliced_fixture(false)?;
    let path_inclusions = cordial::SourceDatabase::load(fixture.path())
        .into_diagnostic()?
        .project_path_inclusions()
        .into_diagnostic()?;
    let source = fs::read_to_string(&owner_file).into_diagnostic()?;
    let findings = scan_derives_rust_source(
        &source,
        &owner_file,
        &owner_root.join("src"),
        &owner_root,
        DerivesThresholds::default(),
        &path_inclusions,
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings
            .iter()
            .all(|record| record.rule_id() != DeriveRuleId::Getter001),
        "consumer splices this file in without derive_getters, so the \
         recommendation isn't actually satisfiable everywhere: {findings:?}"
    );
    Ok(())
}

#[test]
fn path_spliced_file_with_the_dependency_is_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, owner_root, owner_file) = write_path_spliced_fixture(true)?;
    let path_inclusions = cordial::SourceDatabase::load(fixture.path())
        .into_diagnostic()?
        .project_path_inclusions()
        .into_diagnostic()?;
    let source = fs::read_to_string(&owner_file).into_diagnostic()?;
    let findings = scan_derives_rust_source(
        &source,
        &owner_file,
        &owner_root.join("src"),
        &owner_root,
        DerivesThresholds::default(),
        &path_inclusions,
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(
        findings
            .iter()
            .any(|record| record.rule_id() == DeriveRuleId::Getter001),
        "consumer has derive_getters available, so adding the dependency \
         should be the only difference from the exempt fixture and the \
         finding should still fire: {findings:?}"
    );
    Ok(())
}

#[test]
fn clap_schema_skips_pub_field() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
#[derive(Parser)]
pub struct Cli {
    pub project: Option<String>,
    pub force: bool,
}

#[derive(Args)]
pub struct QualityArgs {
    pub apply: bool,
}

pub struct Record {
    pub name: String,
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let pub_fields: Vec<&str> = findings
        .iter()
        .filter(|record| record.rule_id() == DeriveRuleId::PubField001)
        .map(|record| record.struct_name().as_str())
        .collect();
    assert!(
        !pub_fields
            .iter()
            .any(|name| *name == "Cli" || *name == "QualityArgs"),
        "clap Parser/Args skip DERIVE-PUB-FIELD-001: {pub_fields:?}"
    );
    assert!(
        pub_fields.contains(&"Record"),
        "non-clap pub fields still flag: {pub_fields:?}"
    );
    Ok(())
}

/// `#[cfg(creusot)]`-only structs are exempt from `DERIVE-PUB-FIELD-001`:
/// Creusot's own proof-transparency check requires everything an
/// `#[ensures(..)]`/`#[requires(..)]` clause touches to be at least as
/// visible as the function stating it -- confirmed against a real `cargo
/// creusot` translation (not assumed): privatizing
/// `amenable_creusot::ledger::ledger_validate::Ledger::balance`, whose
/// own `check_sufficient_funds` method states `#[ensures(..
/// self.balance ..)]`, produced a real "cannot make `.. balance`
/// transparent in `.. check_sufficient_funds`" translation error.
///
/// The attribute sits directly on the struct here -- covers
/// `is_cfg_creusot`'s own item-level check. The real motivating shape
/// (the gate on an *enclosing* `mod` instead) is a separate test below,
/// `cfg_creusot_mod_skips_pub_field_on_ungated_struct_inside_it`: the
/// two are genuinely different code paths
/// (`is_cfg_creusot(&item_struct.attrs)` vs.
/// `DeriveScanVisitor::in_cfg_creusot_mod`) and a bug in one doesn't
/// imply the other works.
#[test]
fn cfg_creusot_struct_skips_pub_field() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
#[cfg(creusot)]
pub struct Ledger {
    pub balance: i64,
}

pub struct Record {
    pub name: String,
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let pub_fields: Vec<&str> = findings
        .iter()
        .filter(|record| record.rule_id() == DeriveRuleId::PubField001)
        .map(|record| record.struct_name().as_str())
        .collect();
    assert!(
        !pub_fields.contains(&"Ledger"),
        "cfg(creusot) structs skip DERIVE-PUB-FIELD-001: {pub_fields:?}"
    );
    assert!(
        pub_fields.contains(&"Record"),
        "non-creusot pub fields still flag: {pub_fields:?}"
    );
    Ok(())
}

/// The real, predominant shape in `amenable_creusot`: the gate sits on
/// an enclosing `mod`, never repeated on the struct itself (grep
/// confirms zero structs anywhere in that crate carry `#[cfg(creusot)]`
/// directly). `is_cfg_creusot` alone -- checking only an item's own
/// attrs -- never matches this shape at all; this exercises
/// `DeriveScanVisitor::in_cfg_creusot_mod`, the ancestor-tracking fix,
/// against the exact real site
/// (`amenable_creusot::ledger::ledger_validate::mirror::Ledger`) that
/// motivated `is_cfg_creusot` in the first place but which the
/// item-level check never actually covered.
#[test]
fn cfg_creusot_mod_skips_pub_field_on_ungated_struct_inside_it() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
#[cfg(creusot)]
mod mirror {
    pub struct Ledger {
        pub(crate) balance: i64,
    }
}

pub struct Record {
    pub name: String,
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let pub_fields: Vec<&str> = findings
        .iter()
        .filter(|record| record.rule_id() == DeriveRuleId::PubField001)
        .map(|record| record.struct_name().as_str())
        .collect();
    assert!(
        !pub_fields.contains(&"Ledger"),
        "a struct inside a cfg(creusot) mod skips DERIVE-PUB-FIELD-001 even without its own attribute: {pub_fields:?}"
    );
    assert!(
        pub_fields.contains(&"Record"),
        "non-creusot pub fields still flag: {pub_fields:?}"
    );
    Ok(())
}

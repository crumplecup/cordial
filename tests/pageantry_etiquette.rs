use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    PAGEANTRY_ETIQUETTE, PageantryRuleId, RunAll, Session, SessionBuilder, scan_crate_pageantry,
    scan_pageantry_rust_source,
};

fn scan(source: &str) -> miette::Result<Vec<PageantryRuleId>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;
    let records = scan_pageantry_rust_source(source, &file, fixture.path(), fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    Ok(records.into_iter().map(|record| record.rule_id).collect())
}

#[test]
fn leading_trait_block_is_fine() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
use std::fmt::Debug;

mod inner;

pub trait First {}
pub trait Second {}

pub struct Alpha;
pub struct Beta;
"#,
    )?;
    assert!(ids.is_empty());
    Ok(())
}

#[test]
fn types_then_trait_then_types_fires() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
pub struct Alpha;
pub struct Beta;

pub trait Middle {}

pub struct Gamma;
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Trait001]);
    Ok(())
}

#[test]
fn types_then_trait_at_end_fires() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
pub struct Alpha;
pub struct Beta;

pub trait Late {}
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Trait001]);
    Ok(())
}

#[test]
fn trait_after_types_then_another_trait_fires_on_the_second() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
pub trait Early {}

pub struct Alpha;

pub trait Late {}
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Trait001]);
    Ok(())
}

#[test]
fn impl_between_traits_ends_the_leading_block() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
pub trait First {}

impl First for u8 {}

pub trait Second {}
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Trait001]);
    Ok(())
}

#[test]
fn cfg_test_sandwich_is_skipped() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan(
        r#"
pub struct Alpha;

#[cfg(test)]
pub trait OnlyInTests {}

pub struct Beta;
"#,
    )?;
    assert!(ids.is_empty());
    Ok(())
}

#[test]
fn inline_mod_has_its_own_item_list() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    let source = r#"
pub trait FileLevel {}

pub struct FileType;

mod nested {
    pub struct Inner;

    pub trait Buried {}
}
"#;
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;
    let records = scan_pageantry_rust_source(source, &file, fixture.path(), fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].rule_id, PageantryRuleId::Trait001);
    assert_eq!(records[0].context, "sample::nested");
    assert_eq!(records[0].snippet, "trait Buried");
    Ok(())
}

#[test]
fn pageantry_etiquette_writes_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub struct Alpha;
pub struct Beta;

pub trait Middle {}

pub struct Gamma;
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PAGEANTRY_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 1);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("pageantry.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("PAGEANTRY-TRAIT-001"));
    assert!(csv.contains("trait Middle"));

    let checklist = fs::read_to_string(findings_dir.join("pageantry.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 1"));
    Ok(())
}

#[test]
fn dogfood_cordial_traits_are_at_the_top() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let records = scan_crate_pageantry(root)
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    let listed: Vec<String> = records
        .iter()
        .map(|record| {
            format!(
                "{}:{} {} ({})",
                record.file.display(),
                record.line,
                record.snippet,
                record.context
            )
        })
        .collect();
    assert!(
        listed.is_empty(),
        "cordial traits should sit in a leading block:\n{}",
        listed.join("\n")
    );
    Ok(())
}

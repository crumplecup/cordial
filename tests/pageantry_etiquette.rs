use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    PAGEANTRY_ETIQUETTE, PageantryRuleId, PageantryThresholds, RunAll, Session, SessionBuilder,
    load_cordial_config, scan_crate_pageantry, scan_pageantry_rust_source,
};

/// `[pageantry]` knobs as a project would write them in `cordial.toml`.
fn thresholds_from_toml(body: &str) -> miette::Result<PageantryThresholds> {
    let workspace = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    fs::write(workspace.path().join("cordial.toml"), body)
        .into_diagnostic()
        .wrap_err("write cordial.toml")?;
    Ok(*load_cordial_config(workspace.path(), store.path()).pageantry())
}

fn scan(source: &str) -> miette::Result<Vec<PageantryRuleId>> {
    scan_named("sample.rs", source)
}

fn scan_named(name: &str, source: &str) -> miette::Result<Vec<PageantryRuleId>> {
    Ok(scan_records(name, source, &PageantryThresholds::default())?
        .into_iter()
        .map(|(rule_id, _)| rule_id)
        .collect())
}

/// Rule id and snippet for each record, with explicit thresholds.
fn scan_records(
    name: &str,
    source: &str,
    thresholds: &PageantryThresholds,
) -> miette::Result<Vec<(PageantryRuleId, String)>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join(name);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("parent dir")?;
    }
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write")?;
    let records =
        scan_pageantry_rust_source(source, &file, fixture.path(), fixture.path(), thresholds)
            .into_diagnostic()
            .wrap_err("scan")?;
    Ok(records
        .into_iter()
        .map(|record| (record.rule_id(), record.snippet().clone()))
        .collect())
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
    let records = scan_pageantry_rust_source(
        source,
        &file,
        fixture.path(),
        fixture.path(),
        &PageantryThresholds::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].rule_id(), PageantryRuleId::Trait001);
    assert_eq!(records[0].context(), "sample::nested");
    assert_eq!(records[0].snippet(), "trait Buried");
    Ok(())
}

#[test]
fn pageantry_etiquette_writes_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod arrange;\n")
        .into_diagnostic()
        .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("src/arrange.rs"),
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
        .register(&*PAGEANTRY_ETIQUETTE)
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
    let records = scan_crate_pageantry(root, &PageantryThresholds::default())
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    let listed: Vec<String> = records
        .iter()
        .map(|record| {
            format!(
                "{}:{} {} ({})",
                record.file().display(),
                record.line(),
                record.snippet(),
                record.context()
            )
        })
        .collect();
    assert!(
        listed.is_empty(),
        "cordial traits should sit in a leading block, and lib.rs / mod.rs should be barrels:\n{}",
        listed.join("\n")
    );
    Ok(())
}

#[test]
fn barrel_lib_with_a_function_fires() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "lib.rs",
        r#"
mod inner;

pub fn helper() {}
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Barrel001]);
    Ok(())
}

#[test]
fn barrel_mod_with_a_type_fires() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "mod.rs",
        r#"
pub use inner::Alpha;

pub struct Alpha;
"#,
    )?;
    assert_eq!(ids, vec![PageantryRuleId::Barrel001]);
    Ok(())
}

#[test]
fn barrel_only_mods_and_reexports_is_fine() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "lib.rs",
        r#"
mod inner;
pub use inner::Alpha;
"#,
    )?;
    assert!(ids.is_empty());
    Ok(())
}

#[test]
fn barrel_lib_may_hold_proc_macro_entry_points() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "lib.rs",
        r#"
mod expand;

#[proc_macro]
pub fn harness(input: TokenStream) -> TokenStream {
    expand::harness(input)
}

#[proc_macro_derive(Entry, attributes(entry))]
pub fn derive_entry(input: TokenStream) -> TokenStream {
    expand::entry(input)
}

#[proc_macro_attribute]
pub fn calculation(args: TokenStream, input: TokenStream) -> TokenStream {
    expand::calculation(args, input)
}
"#,
    )?;
    assert!(ids.is_empty(), "{ids:?}");
    Ok(())
}

#[test]
fn barrel_lib_still_flags_plain_functions_beside_proc_macros() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "lib.rs",
        r#"
#[proc_macro]
pub fn harness(input: TokenStream) -> TokenStream {
    input
}

pub fn helper() {}

#[inline]
pub fn tagged_helper() {}
"#,
    )?;
    assert_eq!(
        ids,
        vec![PageantryRuleId::Barrel001, PageantryRuleId::Barrel001]
    );
    Ok(())
}

const FAT_ENTRY_POINT: &str = r#"
mod expand;

#[proc_macro_derive(Entry)]
pub fn derive_entry(input: TokenStream) -> TokenStream {
    let ast = parse(input);
    let name = ast.ident();
    let fields = ast.fields();
    let mut out = Vec::new();
    for field in fields {
        out.push(field.render());
    }
    let joined = out.join(",");
    emit(name, joined)
}
"#;

#[test]
fn barrel_lib_flags_a_proc_macro_with_logic_in_its_body() -> miette::Result<()> {
    cordial::init_tracing();
    let records = scan_records("lib.rs", FAT_ENTRY_POINT, &PageantryThresholds::default())?;
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, PageantryRuleId::BarrelShim001);
    // The finding says what and where, not just an id.
    assert!(
        records[0]
            .1
            .contains("#[proc_macro_derive] fn derive_entry")
    );
    assert!(records[0].1.contains("9-line body"));
    assert!(records[0].1.contains("shims may have 8"));
    Ok(())
}

#[test]
fn shim_limit_is_read_from_cordial_toml() -> miette::Result<()> {
    cordial::init_tracing();
    let roomy = thresholds_from_toml("[pageantry]\nmax_shim_lines = 12\n")?;
    assert_eq!(roomy.max_shim_lines(), 12);
    assert!(scan_records("lib.rs", FAT_ENTRY_POINT, &roomy)?.is_empty());

    let strict = thresholds_from_toml("[pageantry]\nmax_shim_lines = 1\n")?;
    let records = scan_records(
        "lib.rs",
        "mod expand;\n\n#[proc_macro]\npub fn harness(input: TokenStream) -> TokenStream {\n    let parsed = parse(input);\n    expand::harness(parsed)\n}\n",
        &strict,
    )?;
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, PageantryRuleId::BarrelShim001);
    Ok(())
}

#[test]
fn single_pageantry_rules_can_be_switched_off() -> miette::Result<()> {
    cordial::init_tracing();
    let no_shim = thresholds_from_toml("[pageantry]\nbarrel_shim = false\n")?;
    assert!(scan_records("lib.rs", FAT_ENTRY_POINT, &no_shim)?.is_empty());

    let no_barrel = thresholds_from_toml("[pageantry]\nbarrel = false\n")?;
    assert!(scan_records("lib.rs", "pub fn helper() {}\n", &no_barrel)?.is_empty());
    assert_eq!(
        scan_records("lib.rs", FAT_ENTRY_POINT, &no_barrel)?.len(),
        1,
        "shim rule is independent of the barrel rule"
    );

    let no_traits = thresholds_from_toml("[pageantry]\ntrait_block = false\n")?;
    let late_trait = "pub struct A;\npub trait Late {}\n";
    assert!(scan_records("sample.rs", late_trait, &no_traits)?.is_empty());
    assert_eq!(
        scan_records("sample.rs", late_trait, &PageantryThresholds::default())?.len(),
        1
    );
    Ok(())
}

#[test]
fn pageantry_enabled_flag_still_gates_the_etiquette() -> miette::Result<()> {
    cordial::init_tracing();
    let off = thresholds_from_toml("[pageantry]\nenabled = false\n")?;
    assert!(!off.enabled());
    assert!(PageantryThresholds::default().enabled());
    Ok(())
}

#[test]
fn named_sibling_may_hold_types() -> miette::Result<()> {
    cordial::init_tracing();
    let ids = scan_named(
        "inner.rs",
        r#"
pub struct Alpha;

pub fn helper() {}
"#,
    )?;
    assert!(ids.is_empty());
    Ok(())
}

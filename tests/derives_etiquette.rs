use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    DERIVES_ETIQUETTE, DeriveRuleId, DeriveSiteRecord, DerivesThresholds, PathInclusionFacts,
    RunAll, Session, SessionBuilder, scan_derives_rust_source,
};

const TRIVIAL_GETTER: &str = r#"struct Widget {
    name: String,
    count: u32,
}

impl Widget {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}
"#;

#[test]
fn derives_etiquette_detects_trivial_getters() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), TRIVIAL_GETTER)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&DERIVES_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("derives.csv"))
        .into_diagnostic()
        .wrap_err("derives csv")?;
    assert!(csv.contains("DERIVE-GETTER-001"));
    assert!(csv.contains("Widget::name"));
    assert!(csv.contains("Widget::count"));

    let checklist = fs::read_to_string(findings_dir.join("derives.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("derive_getters"));

    let summary = fs::read_to_string(findings_dir.join("derives-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("getter **2**"));
    Ok(())
}

#[test]
fn scan_derives_rust_source_flags_trivial_getters() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("trivial_getter.rs");
    fs::write(&file, TRIVIAL_GETTER)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_derives_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        DerivesThresholds::default(),
        &PathInclusionFacts::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings
            .iter()
            .filter(|record| record.rule_id == DeriveRuleId::Getter001)
            .count(),
        2
    );
    Ok(())
}

fn scan_rules(source: &str, thresholds: DerivesThresholds) -> miette::Result<Vec<DeriveRuleId>> {
    Ok(scan_findings(source, thresholds)?
        .into_iter()
        .map(|record| record.rule_id)
        .collect())
}

fn scan_findings(
    source: &str,
    thresholds: DerivesThresholds,
) -> miette::Result<Vec<DeriveSiteRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("sample.rs");
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write sample")?;
    scan_derives_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
        thresholds,
        &PathInclusionFacts::default(),
    )
    .into_diagnostic()
    .wrap_err("scan")
}

#[test]
fn error_type_new_is_exempt_from_derive_new() -> miette::Result<()> {
    let source = r#"
#[derive(Debug)]
struct IoSource {
    message: String,
}

impl IoSource {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::error::Error for IoSource {}
impl std::fmt::Display for IoSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::New001),
        "error types skip derive_new: {findings:?}"
    );
    Ok(())
}

#[test]
fn track_caller_new_is_exempt_from_derive_new() -> miette::Result<()> {
    let source = r#"
struct Located {
    file: String,
}

impl Located {
    #[track_caller]
    pub fn new(file: String) -> Self {
        Self { file }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.iter().all(|rule_id| {
            *rule_id != DeriveRuleId::New001 && *rule_id != DeriveRuleId::UseBuilder001
        }),
        "track_caller constructors skip new/builder arity: {findings:?}"
    );
    Ok(())
}

#[test]
fn new_above_max_args_requires_a_builder() -> miette::Result<()> {
    let source = r#"
struct Point {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
}

impl Point {
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self { a, b, c, d }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert_eq!(
        findings
            .iter()
            .filter(|rule_id| **rule_id == DeriveRuleId::UseBuilder001)
            .count(),
        1
    );
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::New001)
    );
    Ok(())
}

#[test]
fn fat_new_with_computed_fields_is_not_a_builder_candidate() -> miette::Result<()> {
    let source = r#"
struct Segment {
    first_ancestor: u8,
    second_ancestor: u8,
    leaf: u8,
}

impl Segment {
    pub fn new(base: u8, first: u8, second: u8, delta: u8) -> Self {
        let first_ancestor = base + first;
        let second_ancestor = first_ancestor + second;
        let leaf = second_ancestor + delta;
        Self {
            first_ancestor,
            second_ancestor,
            leaf,
        }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "a >3-arg constructor whose fields are computed, not passed \
         straight through, can't be replicated by derive_builder (its \
         setters just assign the struct's own fields) -- must not \
         recommend DERIVE-USE-BUILDER-001 on arg count alone: {findings:?}"
    );
    Ok(())
}

#[test]
fn fat_new_with_a_conditional_is_not_a_builder_candidate() -> miette::Result<()> {
    let source = r#"
struct Ordered {
    low: u8,
    high: u8,
    a: u8,
    b: u8,
}

impl Ordered {
    pub fn new(first: u8, second: u8, a: u8, b: u8) -> Self {
        if first <= second {
            Self { low: first, high: second, a, b }
        } else {
            Self { low: second, high: first, a, b }
        }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "a >3-arg constructor whose tail expression is a conditional, not \
         a bare struct literal, can't be replicated by derive_builder --\
         must not recommend DERIVE-USE-BUILDER-001 on arg count alone: \
         {findings:?}"
    );
    Ok(())
}

#[test]
fn field_from_a_differently_named_param_is_not_a_derive_new_candidate() -> miette::Result<()> {
    let source = r#"
struct Widget {
    path: String,
}

impl Widget {
    pub fn new(dir: String, file_name: String) -> Self {
        Self { path: format!("{dir}/{file_name}") }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "the field is computed from two differently-named parameters, \
         not a trivial pass-through of a same-named one -- derive_new \
         can't replicate this: {findings:?}"
    );
    Ok(())
}

#[test]
fn field_wrapping_its_param_in_another_call_is_not_a_derive_new_candidate() -> miette::Result<()> {
    let source = r#"
struct Widget {
    value: Option<u32>,
}

impl Widget {
    pub fn new(value: u32) -> Self {
        Self { value: Some(value) }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "the field wraps its param in Some(..), not a bare/into/clone \
         pass-through -- derive_new can't replicate this: {findings:?}"
    );
    Ok(())
}

#[test]
fn hardcoded_extra_field_is_not_a_derive_new_candidate() -> miette::Result<()> {
    let source = r#"
struct Widget {
    items: u32,
    cursor: usize,
}

impl Widget {
    pub fn new(items: u32) -> Self {
        Self { items, cursor: 0 }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "`cursor: 0` has no corresponding parameter at all -- derive_new \
         can't replicate a hardcoded field: {findings:?}"
    );
    Ok(())
}

#[test]
fn trivial_new_at_max_args_suggests_derive_new() -> miette::Result<()> {
    let source = r#"
struct Point {
    a: u8,
    b: u8,
    c: u8,
}

impl Point {
    pub fn new(a: u8, b: u8, c: u8) -> Self {
        Self { a, b, c }
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert_eq!(
        findings
            .iter()
            .filter(|rule_id| **rule_id == DeriveRuleId::New001)
            .count(),
        1
    );
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::UseBuilder001)
    );
    Ok(())
}

#[test]
fn hand_rolled_builder_type_is_still_derive_builder() -> miette::Result<()> {
    let source = r#"
struct WidgetBuilder {
    name: String,
}

impl WidgetBuilder {
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn build(self) -> WidgetBuilder {
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.contains(&DeriveRuleId::Builder001),
        "expected DERIVE-BUILDER-001: {findings:?}"
    );
    Ok(())
}

#[test]
fn max_constructor_args_override_keeps_fat_new_as_derive_new() -> miette::Result<()> {
    let source = r#"
struct Point {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
}

impl Point {
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self { a, b, c, d }
    }
}
"#;
    let thresholds = DerivesThresholds::new(4, 2);
    let findings = scan_rules(source, thresholds)?;
    assert_eq!(
        findings
            .iter()
            .filter(|rule_id| **rule_id == DeriveRuleId::New001)
            .count(),
        1
    );
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::UseBuilder001)
    );
    Ok(())
}

#[test]
fn clone_getter_recommends_copy_getters() -> miette::Result<()> {
    let source = r#"
struct Widget {
    name: String,
}

impl Widget {
    pub fn name(&self) -> String {
        self.name.clone()
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let getter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Getter001)
        .expect("clone getter should flag DERIVE-GETTER-001");
    assert!(
        getter.recommendation.contains("getter(copy)"),
        "owned getter should steer to #[getter(copy)]: {}",
        getter.recommendation
    );
    Ok(())
}

#[test]
fn bare_field_return_recommends_copy_getters() -> miette::Result<()> {
    let source = r#"
struct Widget {
    id: u32,
}

impl Widget {
    pub fn id(&self) -> u32 {
        self.id
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let getter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Getter001)
        .expect("bare self.field return should flag DERIVE-GETTER-001");
    assert!(
        getter.recommendation.contains("getter(copy)"),
        "returning the field by value (not `&self.field`) only compiles \
         because it's Copy, and a plain #[derive(Getters)] would return a \
         reference instead -- must steer to #[getter(copy)]: {}",
        getter.recommendation
    );
    Ok(())
}

#[test]
fn reference_field_return_does_not_recommend_copy_getters() -> miette::Result<()> {
    let source = r#"
struct Widget {
    id: u32,
}

impl Widget {
    pub fn id(&self) -> &u32 {
        &self.id
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let getter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Getter001)
        .expect("&self.field return should flag DERIVE-GETTER-001");
    assert!(
        !getter.recommendation.contains("getter(copy)"),
        "returning `&self.field` is exactly what a plain #[derive(Getters)] \
         already produces -- must NOT steer to #[getter(copy)], which would \
         change the return type from &u32 to u32: {}",
        getter.recommendation
    );
    Ok(())
}

#[test]
fn trivial_setter_is_flagged() -> miette::Result<()> {
    let source = r#"
struct Point {
    x: u32,
}

impl Point {
    pub fn with_x(mut self, x: u32) -> Self {
        self.x = x;
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert_eq!(
        findings
            .iter()
            .filter(|rule_id| **rule_id == DeriveRuleId::Setter001)
            .count(),
        1,
        "expected DERIVE-SETTER-001: {findings:?}"
    );
    Ok(())
}

#[test]
fn into_only_setter_is_still_flagged() -> miette::Result<()> {
    let source = r#"
struct Widget {
    name: String,
}

impl Widget {
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let setter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Setter001)
        .expect("into setter should flag DERIVE-SETTER-001");
    assert!(
        setter.recommendation.contains("into"),
        "into() should steer to #[setters(into)]: {}",
        setter.recommendation
    );
    Ok(())
}

#[test]
fn wrapping_some_setter_recommends_strip_option() -> miette::Result<()> {
    let source = r#"
struct Node {
    name: Option<String>,
}

impl Node {
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let setter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Setter001)
        .expect("Some(arg) should flag DERIVE-SETTER-001");
    assert!(
        setter.recommendation.contains("strip_option"),
        "Some(arg) should steer to #[setters(strip_option)]: {}",
        setter.recommendation
    );
    Ok(())
}

#[test]
fn two_wrapping_fluents_are_a_builder() -> miette::Result<()> {
    let source = r#"
struct Node {
    name: Option<String>,
    span: Option<u32>,
}

impl Node {
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_span(mut self, span: u32) -> Self {
        self.span = Some(span);
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.contains(&DeriveRuleId::Builder001),
        "strip_option fluents count toward min_fluent_setters: {findings:?}"
    );
    Ok(())
}

#[test]
fn two_trivial_fluents_are_a_builder() -> miette::Result<()> {
    let source = r#"
struct Point {
    x: u32,
    y: u32,
}

impl Point {
    pub fn with_x(mut self, x: u32) -> Self {
        self.x = x;
        self
    }

    pub fn with_y(mut self, y: u32) -> Self {
        self.y = y;
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.contains(&DeriveRuleId::Builder001),
        "expected DERIVE-BUILDER-001 from two trivial fluents: {findings:?}"
    );
    Ok(())
}

#[test]
fn some_into_setter_recommends_both_options() -> miette::Result<()> {
    let source = r#"
struct Node {
    name: Option<String>,
}

impl Node {
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}
"#;
    let findings = scan_findings(source, DerivesThresholds::default())?;
    let setter = findings
        .iter()
        .find(|record| record.rule_id == DeriveRuleId::Setter001)
        .expect("Some(arg.into()) should flag DERIVE-SETTER-001");
    assert!(
        setter.recommendation.contains("strip_option") && setter.recommendation.contains("into"),
        "expected strip_option and into: {}",
        setter.recommendation
    );
    Ok(())
}

#[test]
fn as_str_forwards_to_derive_more_as_ref() -> miette::Result<()> {
    let source = r#"
struct Widget {
    name: String,
}

impl Widget {
    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.contains(&DeriveRuleId::AsStr001),
        "expected DERIVE-ASSTR-001: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::Getter001),
        "as_str is not a getters derive: {findings:?}"
    );
    Ok(())
}

#[test]
fn as_ref_forwards_to_derive_more_as_ref() -> miette::Result<()> {
    let source = r#"
struct Widget {
    name: String,
}

impl Widget {
    pub fn as_ref(&self) -> &str {
        self.name.as_ref()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.contains(&DeriveRuleId::AsRef001),
        "expected DERIVE-ASREF-001: {findings:?}"
    );
    Ok(())
}

#[test]
fn const_fn_new_getter_setter_and_as_ref_are_all_exempt() -> miette::Result<()> {
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
    let (fixture, owner_root, owner_file) = write_path_spliced_fixture(false)?;
    let path_inclusions = cordial::workspace_path_inclusions(fixture.path());
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
            .all(|record| record.rule_id != DeriveRuleId::Getter001),
        "consumer splices this file in without derive_getters, so the \
         recommendation isn't actually satisfiable everywhere: {findings:?}"
    );
    Ok(())
}

#[test]
fn path_spliced_file_with_the_dependency_is_still_flagged() -> miette::Result<()> {
    let (fixture, owner_root, owner_file) = write_path_spliced_fixture(true)?;
    let path_inclusions = cordial::workspace_path_inclusions(fixture.path());
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
            .any(|record| record.rule_id == DeriveRuleId::Getter001),
        "consumer has derive_getters available, so adding the dependency \
         should be the only difference from the exempt fixture and the \
         finding should still fire: {findings:?}"
    );
    Ok(())
}

#[test]
fn existing_as_ref_derive_skips_as_str() -> miette::Result<()> {
    let source = r#"
#[derive(derive_more::AsRef)]
struct Widget {
    name: String,
}

impl Widget {
    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings
            .iter()
            .all(|rule_id| *rule_id != DeriveRuleId::AsStr001),
        "AsRef derive skips as_str: {findings:?}"
    );
    Ok(())
}

#[test]
fn clap_schema_skips_pub_field() -> miette::Result<()> {
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
        .filter(|record| record.rule_id == DeriveRuleId::PubField001)
        .map(|record| record.struct_name.as_str())
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

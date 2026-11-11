//! Setters, fluent setters and `AsRef` forwarding: what is trivial, what wraps, and what derives.

use cordial::{DeriveRuleId, DerivesThresholds};

#[path = "support/derives_scan.rs"]
mod derives_scan;

#[path = "support/derives_rules.rs"]
mod derives_rules;

use derives_rules::scan_rules;
use derives_scan::scan_findings;

#[test]
fn trivial_setter_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
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
    cordial::init_tracing();
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
        .find(|record| record.rule_id() == DeriveRuleId::Setter001)
        .ok_or_else(|| miette::miette!("into setter should flag DERIVE-SETTER-001"))?;
    assert!(
        setter.recommendation().contains("into"),
        "into() should steer to #[setters(into)]: {}",
        setter.recommendation()
    );
    Ok(())
}

#[test]
fn wrapping_some_setter_recommends_strip_option() -> miette::Result<()> {
    cordial::init_tracing();
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
        .find(|record| record.rule_id() == DeriveRuleId::Setter001)
        .ok_or_else(|| miette::miette!("Some(arg) should flag DERIVE-SETTER-001"))?;
    assert!(
        setter.recommendation().contains("strip_option"),
        "Some(arg) should steer to #[setters(strip_option)]: {}",
        setter.recommendation()
    );
    Ok(())
}

#[test]
fn two_wrapping_fluents_are_a_builder() -> miette::Result<()> {
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
        .find(|record| record.rule_id() == DeriveRuleId::Setter001)
        .ok_or_else(|| miette::miette!("Some(arg.into()) should flag DERIVE-SETTER-001"))?;
    assert!(
        setter.recommendation().contains("strip_option")
            && setter.recommendation().contains("into"),
        "expected strip_option and into: {}",
        setter.recommendation()
    );
    Ok(())
}

#[test]
fn as_str_forwards_to_derive_more_as_ref() -> miette::Result<()> {
    cordial::init_tracing();
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
    cordial::init_tracing();
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
fn option_as_ref_is_not_an_as_ref_trait_candidate() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Widget {
    low_value: Option<u32>,
}

impl Widget {
    pub fn first_item(&self) -> Option<&u32> {
        self.low_value.as_ref()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        findings.is_empty(),
        "Option<T>::as_ref (&Option<T> -> Option<&T>) is a real, \
         distinct std method -- not a derive_more::AsRef-style field \
         forward (&Self -> &FieldType), which returns a completely \
         different shape: {findings:?}"
    );
    Ok(())
}

#[test]
fn existing_as_ref_derive_skips_as_str() -> miette::Result<()> {
    cordial::init_tracing();
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

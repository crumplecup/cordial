//! Constructors: when `new` should be `derive_new`, when it needs a builder, and what makes it exempt.

use cordial::{DeriveRuleId, DerivesThresholds};

#[path = "support/derives_scan.rs"]
mod derives_scan;

#[path = "support/derives_rules.rs"]
mod derives_rules;

use derives_rules::scan_rules;

#[test]
fn error_type_new_is_exempt_from_derive_new() -> miette::Result<()> {
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
fn custom_derived_builder_impl_is_not_hand_rolled_builder() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
#[derive(derive_builder::Builder)]
#[builder(name = "WidgetBuilder")]
struct Widget {
    name: String,
}

impl WidgetBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        !findings.contains(&DeriveRuleId::Builder001),
        "derived builder compatibility impl should not be flagged: {findings:?}"
    );
    Ok(())
}

#[test]
fn max_constructor_args_override_keeps_fat_new_as_derive_new() -> miette::Result<()> {
    cordial::init_tracing();
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

/// A hand-rolled `const fn` fluent builder is real evidence the type
/// needs to stay `const`-constructible (e.g. for `inventory::submit!`,
/// which requires a `const`-evaluable value, or a `&'static [T]` array
/// literal via rvalue static promotion) -- `derive_builder` can't
/// generate `const` methods, so recommending it here would be
/// recommending something that doesn't actually replace what the type
/// needs. Real precedent: `amenable_core::link::ExchangeEdgeRecord`.
#[test]
fn const_fn_fluent_builder_is_exempt_from_derive_builder() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Edge {
    x: u32,
    y: u32,
}

impl Edge {
    pub const fn with_x(mut self, x: u32) -> Self {
        self.x = x;
        self
    }

    pub const fn with_y(mut self, y: u32) -> Self {
        self.y = y;
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        !findings.contains(&DeriveRuleId::Builder001),
        "const fn fluent setters can't be replaced by derive_builder: {findings:?}"
    );
    Ok(())
}

/// A `const fn build(self)` is the same real exemption as the fluent-setter
/// case above, just on the terminal method instead of the setters.
#[test]
fn const_fn_build_is_exempt_from_derive_builder() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Edge {
    x: u32,
}

impl Edge {
    pub const fn build(self) -> Self {
        self
    }
}
"#;
    let findings = scan_rules(source, DerivesThresholds::default())?;
    assert!(
        !findings.contains(&DeriveRuleId::Builder001),
        "const fn build can't be replaced by derive_builder: {findings:?}"
    );
    Ok(())
}

/// Only when *every* fluent setter is `const` does the exemption apply --
/// a mix means the type isn't actually held to a whole-chain `const`
/// requirement, so the ordinary recommendation still stands.
#[test]
fn mixed_const_and_non_const_fluents_still_flag_builder() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Edge {
    x: u32,
    y: u32,
}

impl Edge {
    pub const fn with_x(mut self, x: u32) -> Self {
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
        "a non-const fluent setter means the chain isn't really const-bound: {findings:?}"
    );
    Ok(())
}

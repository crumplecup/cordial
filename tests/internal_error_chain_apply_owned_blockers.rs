//! What keeps a field owned or blocks the conversion: shadowing, dynamic values, aliases, macros and patterns.

use cordial::{ErrorConversionSiteKind, ErrorFieldApplyBlockerReason, run_error_file_apply};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, source};

#[test]
fn shadowed_block_scoped_constant_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self {
        const MESSAGE: &'static str = "fixed";
        {
            const MESSAGE: &'static str = "alternate";
            Self { message: String::from(MESSAGE) }
        }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(
        result.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::Initializer
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn shadowed_static_local_alias_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &'static str) -> Self {
        let alias: &'static str = message;
        let alias = format!("{alias}!");
        Self { message: String::from(alias) }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(
        result.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::Initializer
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn untyped_local_alias_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &'static str) -> Self {
        let alias = message;
        Self { message: String::from(alias) }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(
        result.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::Initializer
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn shadowed_constructor_parameter_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &'static str) -> Self {
        let message = format!("{message}!");
        Self { message: String::from(message) }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(
        result.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::Initializer
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn dynamic_constructor_parameter_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &str) -> Self { Self { message: String::from(message) } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(
        result.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::Initializer
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn dynamic_string_field_stays_owned_while_literal_field_converts() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String, context: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &str) -> Self {
        Self { message: String::from(message), context: "fixed".to_string() }
    }
    fn fixed() -> Self {
        Self { message: "literal".to_string(), context: "fixed".to_string() }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let result = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 1);
    assert_eq!(result.unresolved(), 1);
    let changed = source(root)?;
    assert!(changed.contains("message: String, context: &'static str"));
    assert!(changed.contains("message: String::from(message), context: \"fixed\""));
    assert!(changed.contains("message: \"literal\".to_string(), context: \"fixed\""));
    check_fixture(root)?;
    Ok(())
}

#[test]
fn unrecognized_initializer_skips_entire_error_type() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl std::fmt::Debug for SiteError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) } }
impl std::fmt::Display for SiteError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) } }
impl SiteError {
    fn new(file: &str) -> Self { Self { file: String::from(file) } }
    fn caller() -> Self { Self { file: Location::caller().file().to_string() } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn public_error_field_is_not_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
pub struct SiteError { pub file: String }
impl std::error::Error for SiteError {}
impl SiteError { fn new() -> Self { Self { file: Location::caller().file().to_string() } } }
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn qualified_constructor_and_shadowed_location_are_not_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let qualified = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl SiteError { fn new() -> Self { crate::SiteError { file: Location::caller().file().to_string() } } }
"#;
    let fixture = fixture(qualified, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, qualified);

    let shadowed = r#"use std::panic::Location;
struct SiteError { file: String }
impl std::error::Error for SiteError {}
impl SiteError {
    fn new() -> Self {
        let loc = Location::caller();
        {
            let loc = other_location();
            Self { file: loc.file().to_string() }
        }
    }
}
"#;
    fs::write(fixture.path().join("src/lib.rs"), shadowed).into_diagnostic()?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, shadowed);
    Ok(())
}

#[test]
fn field_use_that_needs_string_blocks_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_string() } }
    fn extend(&mut self) { self.message.push_str(" more"); }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let preview =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 0);
    assert_eq!(preview.unresolved(), 1);
    assert!(preview.conversion_units().iter().any(|unit| {
        unit.sites()
            .iter()
            .any(|site| site.kind() == ErrorConversionSiteKind::FieldUse && !site.resolved())
    }));
    assert_eq!(source(fixture.path())?, original);

    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn owned_return_and_struct_pattern_block_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let return_source = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: "fixed".to_string() } }
    fn into_message(self) -> String { self.message }
}
"#;
    let fixture = fixture(return_source, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, return_source);

    let pattern_source = return_source.replace(
        "fn into_message(self) -> String { self.message }",
        "fn into_message(self) -> String { let Self { message } = self; message }",
    );
    fs::write(fixture.path().join("src/lib.rs"), &pattern_source).into_diagnostic()?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, pattern_source);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn constructor_hidden_in_macro_blocks_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MessageError {}
macro_rules! make_error { () => { MessageError { message: "macro".to_string() } }; }
impl MessageError {
    fn new() -> Self { Self { message: "direct".to_string() } }
    fn from_macro() -> Self { make_error!() }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let result =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0);
    assert_eq!(result.unresolved(), 1);
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn local_name_shadows_and_constructor_aliases_block_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let base = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: String::from("fixed") } }
}
"#;
    let fixture = fixture(base, "borrowed")?;
    let cases = [
        format!("type String = std::string::String;\n{base}"),
        format!(
            r#"#[derive(Debug)]
struct String(&'static str);
impl From<&'static str> for String {{ fn from(value: &'static str) -> Self {{ Self(value) }} }}
impl std::fmt::Display for String {{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{ write!(f, "{{}}", self.0) }}
}}
{base}"#
        ),
        format!("use std::string::String as RenamedString;\ntype String = RenamedString;\n{base}"),
        format!(
            "{base}\ntype Alias = MessageError;\nimpl Alias {{ fn alias() -> Self {{ Alias {{ message: String::from(\"alias\") }} }} }}\n"
        ),
        format!("{base}\nuse self::MessageError as Alias;\n"),
        format!(
            "macro_rules! write {{ ($f:expr, $fmt:literal, $v:expr) => {{ std::write!($f, $fmt, $v) }}; }}\n{base}"
        ),
    ];
    for candidate in cases {
        fs::write(fixture.path().join("src/lib.rs"), &candidate).into_diagnostic()?;
        let summary =
            run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
        assert_eq!(summary.converted_fields(), 0, "{candidate}");
        assert_eq!(summary.unresolved(), 1, "{candidate}");
        assert_eq!(summary.blockers()[0].type_name(), "MessageError");
        assert_eq!(summary.blockers()[0].field_name(), "message");
        assert_eq!(
            summary.blockers()[0].reason(),
            ErrorFieldApplyBlockerReason::AmbiguousName
        );
        assert_eq!(source(fixture.path())?, candidate);
        check_fixture(fixture.path())?;
    }
    Ok(())
}

#[test]
fn local_location_shadow_blocks_call_site_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"struct Location;
impl Location {
    fn caller() -> Self { Self }
    fn file(&self) -> &'static str { "custom" }
}
#[derive(Debug)]
struct SiteError { file: String }
impl std::fmt::Display for SiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for SiteError {}
impl SiteError {
    fn new() -> Self { Self { file: Location::caller().file().to_string() } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.converted_fields(), 0);
    assert_eq!(
        summary.blockers()[0].reason(),
        ErrorFieldApplyBlockerReason::AmbiguousName
    );
    assert_eq!(source(fixture.path())?, original);
    check_fixture(fixture.path())?;
    Ok(())
}

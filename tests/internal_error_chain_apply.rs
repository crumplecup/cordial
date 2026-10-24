use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::{ErrorConversionSiteKind, ErrorFieldApplyBlockerReason, run_error_file_apply};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn conversion_unit_records_constructors_callers_and_error_edges() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String, source: std::io::Error }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(source: std::io::Error) -> Self {
        Self { message: "fixed".to_owned(), source }
    }
}
impl From<std::io::Error> for MessageError {
    fn from(source: std::io::Error) -> Self { Self::new(source) }
}
fn caller() -> Result<(), MessageError> {
    std::fs::read("missing")?;
    let _ = MessageError::new(std::io::Error::other("x"));
    Ok(())
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    let kinds: Vec<_> = unit.sites().iter().map(|site| site.kind()).collect();
    for kind in [
        ErrorConversionSiteKind::Constructor,
        ErrorConversionSiteKind::Caller,
        ErrorConversionSiteKind::TypeUse,
        ErrorConversionSiteKind::FromConversion,
        ErrorConversionSiteKind::TryConversion,
        ErrorConversionSiteKind::ErrorEdge,
    ] {
        assert!(kinds.contains(&kind), "missing {kind:?}: {kinds:?}");
    }
    assert!(!unit.is_closed(), "the `?` conversion is not resolved");
    assert!(unit.sites().iter().all(|site| site.line() > 0));
    assert!(unit.sites().iter().any(|site| {
        site.kind() == ErrorConversionSiteKind::ErrorEdge
            && site.related_type().as_deref() == Some("std::io::Error")
    }));
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn conversion_unit_locates_cross_file_and_macro_paths() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "fixed".to_owned() } } }
macro_rules! hidden { () => { MessageError { message: "hidden".to_owned() } }; }
mod sibling;
"#;
    let fixture = fixture(original, "borrowed")?;
    fs::write(
        fixture.path().join("src/sibling.rs"),
        "use crate::MessageError;\nfn caller() { let _ = MessageError::new(); }\n",
    )
    .into_diagnostic()?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    assert!(
        unit.sites()
            .iter()
            .any(|site| site.kind() == ErrorConversionSiteKind::Macro && !site.resolved())
    );
    assert!(
        unit.sites()
            .iter()
            .any(|site| site.kind() == ErrorConversionSiteKind::Caller
                && site.path().ends_with("sibling.rs")
                && !site.resolved())
    );
    assert!(!unit.is_closed());
    assert_eq!(summary.changed_files(), 0);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}

#[test]
fn one_file_visit_keeps_sites_separate_for_multiple_error_fields() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "message".to_owned() } } }

#[derive(Debug)]
struct DetailError { detail: String }
impl std::fmt::Display for DetailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.detail) }
}
impl std::error::Error for DetailError {}
impl DetailError { fn new() -> Self { Self { detail: "detail".to_owned() } } }

fn callers() { let _ = MessageError::new(); let _ = DetailError::new(); }
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(summary.conversion_units().len(), 2);
    for (type_name, field_name) in [("MessageError", "message"), ("DetailError", "detail")] {
        let unit = summary
            .conversion_units()
            .iter()
            .find(|unit| unit.type_name() == type_name && unit.field_name() == field_name)
            .wrap_err("candidate conversion unit")?;
        assert_eq!(
            unit.sites()
                .iter()
                .filter(|site| site.kind() == ErrorConversionSiteKind::Constructor)
                .count(),
            1
        );
        assert_eq!(
            unit.sites()
                .iter()
                .filter(|site| site.kind() == ErrorConversionSiteKind::Caller)
                .count(),
            1
        );
    }
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}
#[test]
fn generated_default_constructor_is_inventory_blocker() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug, Default)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for MessageError {}
impl MessageError { fn new() -> Self { Self { message: "fixed".to_owned() } } }
"#;
    let fixture = fixture(original, "borrowed")?;
    let summary =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    let unit = summary
        .conversion_units()
        .iter()
        .find(|unit| unit.field_name() == "message")
        .wrap_err("message conversion unit")?;
    assert!(unit.sites().iter().any(|site| site.kind()
        == ErrorConversionSiteKind::GeneratedConstructor
        && !site.resolved()));
    assert_eq!(summary.changed_files(), 0);
    assert_eq!(source(fixture.path())?, original);
    Ok(())
}
#[test]
fn new_error_lifetime_round_trips_when_conversion_unit_is_closed() -> miette::Result<()> {
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
    fn new<'a>(message: &'a str) -> Self {
        Self { message: String::from(message) }
    }
}
fn caller() {
    let _ = MessageError::new("hello");
}
fn make<'a>(message: &'a str) -> MessageError {
    MessageError::new(message)
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let preview = run_error_file_apply(root, root, None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 1);
    assert_eq!(source(root)?, original);
    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("struct MessageError<'a> { message: &'a str }"));
    assert!(changed.contains("impl<'a> MessageError<'a>"));
    assert!(changed.contains("fn new(message: &'a str)"));
    assert!(changed.contains("fn make<'a>(message: &'a str) -> MessageError<'a>"));
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("struct MessageError { message: String }"));
    assert!(changed.contains("impl MessageError"));
    assert!(changed.contains("fn new<'a>(message: &'a str)"));
    assert!(changed.contains("fn make<'a>(message: &'a str) -> MessageError"));
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );
    Ok(())
}

#[test]
fn new_lifetime_declines_explicit_type_use_outside_impls() -> miette::Result<()> {
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
    fn new<'a>(message: &'a str) -> Self {
        Self { message: String::from(message) }
    }
}
fn make(message: &str) -> MessageError {
    MessageError::new(message)
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
fn new_lifetime_declines_return_that_does_not_forward_input() -> miette::Result<()> {
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
    fn new<'a>(message: &'a str) -> Self {
        Self { message: String::from(message) }
    }
}
fn make<'a>(message: &'a str) -> MessageError {
    let _ = message;
    MessageError::new("fixed")
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

fn fixture(source: &str, strategy: &str) -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"file-apply-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), source).into_diagnostic()?;
    set_strategy(root.path(), strategy)?;
    Ok(root)
}

fn set_strategy(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
    .wrap_err("write strategy")
}

fn source(root: &Path) -> miette::Result<String> {
    fs::read_to_string(root.join("src/lib.rs")).into_diagnostic()
}

fn check_fixture(root: &Path) -> miette::Result<()> {
    let result = Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

#[test]
fn call_site_file_round_trips_between_strategies() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
struct SiteError { file: String, line: u32 }
impl std::fmt::Debug for SiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

impl std::fmt::Display for SiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}
impl std::error::Error for SiteError {}
impl SiteError {
    #[track_caller]
    fn new() -> Self {
        let loc = Location::caller();
        Self { file: loc.file().to_string(), line: loc.line() }
    }
    #[track_caller]
    fn direct() -> Self {
        Self { file: Location::caller().file().to_owned(), line: 1 }
    }
    #[track_caller]
    fn inferred() -> Self {
        let loc = Location::caller();
        Self { file: loc.file().into(), line: loc.line() }
    }
    #[track_caller]
    fn from_direct() -> Self {
        Self { file: String::from(Location::caller().file()), line: 2 }
    }
    #[track_caller]
    fn from_alias() -> Self {
        let loc = Location::caller();
        Self { file: std::string::String::from(loc.file()), line: loc.line() }
    }
    #[track_caller]
    fn grouped() -> Self {
        Self { file: (String::from((Location::caller().file()))), line: 3 }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let preview = run_error_file_apply(root, root, None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 1);
    assert_eq!(source(root)?, original);

    let applied = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(applied.converted_fields(), 1);
    assert_eq!(applied.unresolved(), 0);
    let borrowed = source(root)?;
    assert!(borrowed.contains("file: &'static str"), "{borrowed}");
    assert_eq!(borrowed.matches("file: loc.file()").count(), 3);
    assert_eq!(
        borrowed.matches("file: Location::caller().file()").count(),
        3
    );
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let reverse = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(reverse.converted_fields(), 1);
    let owned = source(root)?;
    assert_eq!(owned.matches("file: loc.file().to_string()").count(), 3);
    assert_eq!(
        owned
            .matches("file: Location::caller().file().to_string()")
            .count(),
        3
    );
    check_fixture(root)?;
    Ok(())
}

#[test]
fn literal_string_fields_round_trip_between_strategies() -> miette::Result<()> {
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
    fn first() -> Self {
        Self { message: "first".to_string(), context: "fixed".to_owned() }
    }
    fn second() -> Self {
        Self { message: "second".into(), context: "fixed".to_string() }
    }
    fn third() -> Self {
        Self { message: String::from("third"), context: std::string::String::from("fixed") }
    }
    fn grouped() -> Self {
        Self { message: ("grouped".to_string()), context: String::from(("fixed")) }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let preview = run_error_file_apply(root, root, None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 2);
    assert_eq!(source(root)?, original);

    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 2);
    assert_eq!(borrowed.unresolved(), 0);
    let result = source(root)?;
    assert!(result.contains("message: &'static str, context: &'static str"));
    assert!(result.contains("message: \"first\", context: \"fixed\""));
    assert!(result.contains("message: \"second\", context: \"fixed\""));
    assert!(result.contains("message: \"third\", context: \"fixed\""));
    assert!(result.contains("message: \"grouped\", context: \"fixed\""));
    check_fixture(root)?;

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 2);
    let result = source(root)?;
    assert!(result.contains("message: String, context: String"));
    assert!(result.contains("message: \"first\".to_string(), context: \"fixed\".to_string()"));
    assert!(result.contains("message: \"second\".to_string(), context: \"fixed\".to_string()"));
    assert!(result.contains("message: \"third\".to_string(), context: \"fixed\".to_string()"));
    assert!(result.contains("message: \"grouped\".to_string(), context: \"fixed\".to_string()"));
    check_fixture(root)?;
    Ok(())
}

#[test]
fn qualified_static_string_constants_round_trip() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"const MESSAGE: &'static str = concat!("fix", "ed");
static CONTEXT: &'static str = "site";
#[derive(Debug)]
struct MessageError { message: String, context: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.message)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self {
        Self { message: String::from(self::MESSAGE), context: self::CONTEXT.to_owned() }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let preview =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 2);
    assert_eq!(source(fixture.path())?, original);

    let borrowed =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 2);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: &'static str, context: &'static str"));
    assert!(changed.contains("message: self::MESSAGE, context: self::CONTEXT"));
    check_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(fixture.path(), "owned")?;
    let owned =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 2);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: String, context: String"));
    assert!(
        changed.contains("message: self::MESSAGE.to_string(), context: self::CONTEXT.to_string()")
    );
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn unqualified_constant_is_not_assumed_to_be_the_static_binding() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"const MESSAGE: &'static str = "fixed";
#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: MESSAGE.to_string() } }
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
fn conditional_constant_does_not_prove_static_string_source() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[cfg(any())]
const MESSAGE: &'static str = "other";
#[cfg(not(any()))]
const MESSAGE: &'static str = "fixed";
#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: self::MESSAGE.to_string() } }
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
fn qualified_builtin_macros_round_trip() -> miette::Result<()> {
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
    fn new() -> Self {
        Self { message: String::from(::core::concat!("fix", "ed")), context: ::std::include_str!("context.txt").to_owned() }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    fs::write(fixture.path().join("src/context.txt"), "site").into_diagnostic()?;
    let preview =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 2);
    assert_eq!(source(fixture.path())?, original);

    let borrowed =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 2);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: &'static str, context: &'static str"));
    assert!(changed.contains(
        "message: ::core::concat!(\"fix\", \"ed\"), context: ::std::include_str!(\"context.txt\")"
    ));
    check_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(fixture.path(), "owned")?;
    let owned =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 2);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: String, context: String"));
    assert!(changed.contains("message: ::core::concat!(\"fix\", \"ed\").to_string()"));
    assert!(changed.contains("context: ::std::include_str!(\"context.txt\").to_string()"));
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn unqualified_custom_macro_is_not_assumed_to_be_builtin() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"macro_rules! concat { ($text:literal) => { $text } }
#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new() -> Self { Self { message: String::from(concat!("local")) } }
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
fn static_string_constructor_parameter_round_trips() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new(message: &'static str) -> Self { Self { message: String::from(message) } }
    fn alternate(message: &'static str) -> Self { Self { message: std::string::String::from(message) } }
    fn to_string(message: &'static str) -> Self { Self { message: message.to_string() } }
    fn to_owned(message: &'static str) -> Self { Self { message: message.to_owned() } }
    fn into(message: &'static str) -> Self { Self { message: message.into() } }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let preview =
        run_error_file_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.converted_fields(), 1);
    assert_eq!(source(fixture.path())?, original);

    let borrowed =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: &'static str"));
    assert_eq!(changed.matches("Self { message: message }").count(), 5);
    check_fixture(fixture.path())?;
    assert_eq!(
        run_error_file_apply(fixture.path(), fixture.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(fixture.path(), "owned")?;
    let owned =
        run_error_file_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    let changed = source(fixture.path())?;
    assert!(changed.contains("message: String"));
    assert_eq!(
        changed
            .matches("Self { message: String::from(message) }")
            .count(),
        5
    );
    check_fixture(fixture.path())?;
    Ok(())
}

#[test]
fn typed_static_local_alias_round_trips() -> miette::Result<()> {
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
        Self { message: String::from(alias) }
    }
    fn fixed() -> Self {
        let alias: &'static str = "fixed";
        Self { message: String::from(alias) }
    }
    fn to_string(message: &'static str) -> Self {
        let alias: &'static str = message;
        Self { message: alias.to_string() }
    }
    fn to_owned(message: &'static str) -> Self {
        let alias: &'static str = message;
        Self { message: alias.to_owned() }
    }
    fn into(message: &'static str) -> Self {
        let alias: &'static str = message;
        Self { message: alias.into() }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    assert_eq!(
        run_error_file_apply(root, root, None, true)
            .into_diagnostic()?
            .converted_fields(),
        1
    );
    assert_eq!(source(root)?, original);

    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(root)?;
    assert!(changed.contains("message: &'static str"));
    assert_eq!(changed.matches("Self { message: alias }").count(), 5);
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("message: String"));
    assert_eq!(
        changed
            .matches("Self { message: String::from(alias) }")
            .count(),
        5
    );
    check_fixture(root)?;
    Ok(())
}

#[test]
fn block_scoped_static_string_items_round_trip() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn from_const() -> Self {
        const MESSAGE: &'static str = "fixed";
        Self { message: String::from(MESSAGE) }
    }
    fn from_static() -> Self {
        static MESSAGE: &'static str = "fixed";
        Self { message: MESSAGE.to_owned() }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    assert_eq!(
        run_error_file_apply(root, root, None, true)
            .into_diagnostic()?
            .converted_fields(),
        1
    );
    assert_eq!(source(root)?, original);
    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(root)?;
    assert!(changed.contains("message: &'static str"));
    assert_eq!(changed.matches("Self { message: MESSAGE }").count(), 2);
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("message: String"));
    assert_eq!(
        changed
            .matches("Self { message: String::from(MESSAGE) }")
            .count(),
        2
    );
    check_fixture(root)?;
    Ok(())
}

#[test]
fn existing_error_lifetime_parameter_round_trips() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError<'a> { message: String, context: &'a [u8] }
impl<'a> std::fmt::Display for MessageError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl<'a> std::error::Error for MessageError<'a> {}
impl<'a> MessageError<'a> {
    fn new(message: &'a str, context: &'a [u8]) -> Self {
        Self { message: String::from(message), context }
    }
    fn via_alias(message: &'a str, context: &'a [u8]) -> Self {
        let alias: &'a str = message;
        Self { message: alias.to_string(), context }
    }
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    assert_eq!(
        run_error_file_apply(root, root, None, true)
            .into_diagnostic()?
            .converted_fields(),
        1
    );
    assert_eq!(source(root)?, original);
    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    assert_eq!(borrowed.unresolved(), 0);
    let changed = source(root)?;
    assert!(changed.contains("message: &'a str"));
    assert!(changed.contains("Self { message: message, context }"));
    assert!(changed.contains("Self { message: alias, context }"));
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("message: String"));
    assert!(changed.contains("Self { message: String::from(message), context }"));
    assert!(changed.contains("Self { message: String::from(alias), context }"));
    check_fixture(root)?;
    Ok(())
}

#[test]
fn unrelated_constructor_lifetime_stays_owned() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError<'a> { message: String, context: &'a [u8] }
impl<'a> std::fmt::Display for MessageError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl<'a> std::error::Error for MessageError<'a> {}
impl<'a> MessageError<'a> {
    fn new(message: &str, context: &'a [u8]) -> Self {
        Self { message: String::from(message), context }
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
fn borrowed_string_field_needs_independent_lifetime_anchor() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError<'a> { message: String, context: &'a str }
impl<'a> std::fmt::Display for MessageError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.message)
    }
}
impl<'a> std::error::Error for MessageError<'a> {}
impl<'a> MessageError<'a> {
    fn new(message: &'a str, context: &'a str) -> Self {
        Self { message: String::from(message), context }
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
fn shorthand_field_initializer_round_trips() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError<'a> { message: &'a str, context: &'a [u8] }
impl<'a> std::fmt::Display for MessageError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl<'a> std::error::Error for MessageError<'a> {}
impl<'a> MessageError<'a> {
    fn new(message: &'a str, context: &'a [u8]) -> Self {
        Self { message, context }
    }
}
"#;
    let fixture = fixture(original, "owned")?;
    let root = fixture.path();
    assert_eq!(
        run_error_file_apply(root, root, None, true)
            .into_diagnostic()?
            .converted_fields(),
        1
    );
    assert_eq!(source(root)?, original);
    let result = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 1);
    assert!(source(root)?.contains("Self { message: String::from(message), context }"));
    check_fixture(root)?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    set_strategy(root, "borrowed")?;
    assert_eq!(
        run_error_file_apply(root, root, None, false)
            .into_diagnostic()?
            .converted_fields(),
        1
    );
    assert!(source(root)?.contains("Self { message: message, context }"));
    check_fixture(root)?;
    Ok(())
}

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
fn public_error_type_is_not_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"use std::panic::Location;
pub struct SiteError { file: String }
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

//! Switching the field strategy: call-site file, literal strings and shorthand initialisers round-trip.

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, set_strategy, source};

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

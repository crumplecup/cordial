//! Error lifetimes: when the rewrite adds, keeps or declines a lifetime parameter.

use cordial::{ErrorFieldApplyBlockerReason, run_error_file_apply};
use miette::IntoDiagnostic;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, set_strategy, source};

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
fn error_lifetime_round_trips_with_scalar_context() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String, line: u32 }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.line)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new<'a>(message: &'a str) -> Self {
        Self { message: String::from(message), line: 7 }
    }
}
fn make<'a>(message: &'a str) -> MessageError {
    MessageError::new(message)
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("struct MessageError<'a> { message: &'a str, line: u32 }"));
    assert!(changed.contains("Self { message: message, line: 7 }"));
    check_fixture(root)?;

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    assert!(source(root)?.contains("struct MessageError { message: String, line: u32 }"));
    check_fixture(root)?;
    Ok(())
}

#[test]
fn error_lifetime_round_trips_with_scalar_constructor_argument() -> miette::Result<()> {
    cordial::init_tracing();
    let original = r#"#[derive(Debug)]
struct MessageError { message: String, line: u32 }
impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.line)
    }
}
impl std::error::Error for MessageError {}
impl MessageError {
    fn new<'a>(message: &'a str, line: u32) -> Self {
        Self { message: String::from(message), line }
    }
}
fn caller() {
    let _ = MessageError::new("hello", 7);
}
fn make<'a>(message: &'a str, line: u32) -> MessageError {
    MessageError::new(message, line)
}
"#;
    let fixture = fixture(original, "borrowed")?;
    let root = fixture.path();
    let borrowed = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(borrowed.converted_fields(), 1);
    let changed = source(root)?;
    assert!(changed.contains("struct MessageError<'a> { message: &'a str, line: u32 }"));
    assert!(changed.contains("fn new(message: &'a str, line: u32)"));
    assert!(changed.contains("fn make<'a>(message: &'a str, line: u32) -> MessageError<'a>"));
    check_fixture(root)?;

    set_strategy(root, "owned")?;
    let owned = run_error_file_apply(root, root, None, false).into_diagnostic()?;
    assert_eq!(owned.converted_fields(), 1);
    assert!(source(root)?.contains("struct MessageError { message: String, line: u32 }"));
    assert!(source(root)?.contains("fn make<'a>(message: &'a str, line: u32) -> MessageError"));
    check_fixture(root)?;
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

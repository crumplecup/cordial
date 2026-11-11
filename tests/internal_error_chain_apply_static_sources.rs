//! Static string sources: qualified constants, builtin macros, locals and block-scoped items.

use cordial::{ErrorFieldApplyBlockerReason, run_error_file_apply};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/error_apply_fixture.rs"]
mod error_apply_fixture;

use error_apply_fixture::{check_fixture, fixture, set_strategy, source};

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

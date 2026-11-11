//! Getter consumers: customised getters, unresolved receivers, and known consumers block only their own candidate.
#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/error_getters_fixture.rs"]
mod error_getters_fixture;

use error_getters_fixture::{ORIGINAL, check, configure, fixture};

#[test]
fn customized_getters_and_unresolved_consumers_are_left_unchanged() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    for original in [
        ORIGINAL.replace(
            "    file: String,",
            "    #[getter(rename = \"call_site_file\")]\n    file: String,",
        ),
        ORIGINAL.replace("file: String,", "#[error(source)] file: String,"),
        ORIGINAL.replace("file: String,", "#[error(ignore, source)] file: String,"),
        ORIGINAL
            .replace("file: String,", "#[error(ignore)] file: String,")
            .replace(
                "use derive_more::{Display, Error};",
                "use derive_more::Display;\nuse unknown::Error;",
            ),
        ORIGINAL.replace(
            "    file: String,",
            "    #[cfg(not(kani))]\n    file: String,",
        ),
        ORIGINAL.replace("use derive_getters::Getters;", "use unknown::Getters;"),
        ORIGINAL.replace(
            "use derive_getters::Getters;",
            "mod derive_getters {}\nuse derive_getters::Getters;",
        ),
        format!("{ORIGINAL}\nfn take(error: &Failure) -> &String {{ error.file() }}"),
        format!("{ORIGINAL}\nfn take(error: &Failure) -> &String {{ Failure::file(error) }}"),
        format!("{ORIGINAL}\nunknown!(error.file());"),
        format!(
            "{ORIGINAL}\nfn take(error: &Failure) {{ let file = Failure::file; file(error); }}"
        ),
        format!("{ORIGINAL}\nmod consumer;"),
    ] {
        fs::write(&path, &original).into_diagnostic()?;
        fs::write(
            root.path().join("src/consumer.rs"),
            "fn take(error: &super::Failure) -> &String { error.file() }",
        )
        .into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{original}\n{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    }
    // A user-authored attribute does not belong to Cordial on reverse apply.
    let original = ORIGINAL
        .replace("file: String,", "#[getter(copy)] file: &'static str,")
        .replace("loc.file().to_string()", "loc.file()");
    fs::write(&path, &original).into_diagnostic()?;
    configure(root.path(), "owned")?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 0, "{result:?}");
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn unrelated_receivers_and_shared_harness_body_allow_round_trip() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = format!("{ORIGINAL}\nmod unrelated;\n");
    fs::write(&path, &original).into_diagnostic()?;
    fs::create_dir_all(root.path().join("src/unrelated")).into_diagnostic()?;
    fs::write(
        root.path().join("src/unrelated/declarations.rs"),
        r#"
pub struct Observation { pub file: &'static str }
impl Observation { pub fn file(&self) -> &'static str { self.file } }
pub struct Node;
impl Node { pub fn file(_: &str) -> Self { Self } }
"#,
    )
    .into_diagnostic()?;
    let consumer = r#"
use self::declarations::{Observation, Node};
mod declarations;
impl Observation { pub fn check(self) { assert_eq!(self.file(), "proof.rs"); let entries = vec![format!("{}", self.file), self.file.to_string()]; assert_eq!(entries.len(), 2); } }
pub fn exercise() { Observation { file: "proof.rs" }.check(); let _ = Node::file("proof.rs"); proof(); }
amenable_derive::gallery_harness! { kani, SOURCE, {
    fn proof() {
        #[track_caller]
        fn here() -> &'static std::panic::Location<'static> { std::panic::Location::caller() }
        let location = here();
        assert!(!location.file().is_empty(), "a location has a filename");
    }
} }
"#;
    fs::write(root.path().join("src/unrelated.rs"), consumer).into_diagnostic()?;
    // A minimal proc macro fixture with the same item-forwarding contract.
    fs::create_dir_all(root.path().join("macro-support/src")).into_diagnostic()?;
    fs::write(root.path().join("macro-support/Cargo.toml"), "[package]\nname = \"amenable_derive\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\nproc-macro = true\n").into_diagnostic()?;
    fs::write(root.path().join("macro-support/src/lib.rs"), r#"
#[proc_macro]
pub fn gallery_harness(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match input.into_iter().nth(4) { Some(proc_macro::TokenTree::Group(body)) => body.stream(), _ => proc_macro::TokenStream::new() }
}
"#).into_diagnostic()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(
        &manifest,
        cargo.replace(
            "[dependencies]",
            "[dependencies]\namenable_derive = { path = \"macro-support\" }",
        ),
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("tests/unrelated.rs"),
        "#[test] fn independent_consumers() { getter_apply::exercise(); }",
    )
    .into_diagnostic()?;
    // Export an exercise function without exposing the candidate to consumers.
    let original = format!("{original}pub fn exercise() {{ unrelated::exercise(); }}\n");
    fs::write(&path, &original).into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let before = fs::read(&path).into_diagnostic()?;
        let preview =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert_eq!(preview.converted_fields(), 1, "{preview:?}");
        assert_eq!(fs::read(&path).into_diagnostic()?, before);
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 1, "{applied:?}");
        assert_eq!(
            fs::read_to_string(root.path().join("src/unrelated.rs")).into_diagnostic()?,
            consumer
        );
        check(root.path())?;
        let repeat =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(repeat.converted_fields(), 0, "{repeat:?}");
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn unresolved_receiver_provenance_stays_blocked() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    for consumer in [
        "type Alias = Failure; fn use_alias(value: &Alias) { let _ = value.file(); }",
        "struct Wrapper(Failure); impl std::ops::Deref for Wrapper { type Target = Failure; fn deref(&self) -> &Failure { &self.0 } } fn use_deref(value: &Wrapper) { let _ = value.file(); }",
        "fn shadow(value: &Failure) { let location = std::panic::Location::caller(); let location = value; let _ = location.file(); }",
        "fn shadow_loop(values: &[Failure]) { let location = std::panic::Location::caller(); for location in values { let _ = location.file(); } }",
        "fn shadow_match(value: &Failure) { let location = std::panic::Location::caller(); match Some(value) { Some(location) => { let _ = location.file(); }, _ => {} } }",
        "fn local_alias(value: &Failure) { use crate::Failure as Location; let location: &Location = value; assert!(!location.file().is_empty()); }",
        "fn unknown_macro(value: &Failure) { unknown!(value.file()); }",
        "fn standard_macro(value: &Failure) { let _ = vec![format!(\"{}\", value.file())]; }",
        "fn direct_field(value: &Failure) { let _: &String = &value.file; }",
        "fn field_in_macro(value: &Failure) { let _: Vec<&String> = vec![&value.file]; }",
        "fn nested_field(value: &Failure) { let _ = vec![format!(\"{}\", value.file)]; }",
        "fn shadow_assert(value: &Failure) { macro_rules! assert { ($x:expr) => { let _: &String = $x; } } assert!(value.file()); }",
        "mod inner { use super::Failure as Other; fn take(value: &Other) -> &String { value.file() } }",
        "fn unknown_origin(value: &Failure) { let location = make_location(value); assert!(!location.file().is_empty()); }",
        "amenable_derive::gallery_harness! { kani, SOURCE, { fn proof(value: &Failure) { assert!(!value.file().is_empty()); } } }",
        "other::gallery_harness! { kani, SOURCE, { fn proof() { let location = std::panic::Location::caller(); assert!(!location.file().is_empty()); } } }",
        "extern crate impostor as std; fn shadow(value: &std::panic::Location<'static>) { let _ = value.file(); }",
        "use impostor::assert; fn shadow() { let location = std::panic::Location::caller(); assert!(location.file()); }",
        "use impostor::*; fn shadow(value: &std::panic::Location<'static>) { let _ = value.file(); }",
        "struct Wrapper(Failure); impl std::ops::Deref for Wrapper { type Target = Failure; fn deref(&self) -> &Failure { &self.0 } } impl Wrapper { #[cfg(any())] fn file(&self) -> &'static str { \"\" } } fn conditional(value: &Wrapper) { let _ = value.file(); }",
        "struct Wrapper(Failure); impl std::ops::Deref for Wrapper { type Target = Failure; fn deref(&self) -> &Failure { &self.0 } } impl Wrapper { fn file() -> &'static str { \"\" } } fn associated_is_not_method(value: &Wrapper) { let _ = value.file(); }",
        "struct Wrapper<T>(Failure, T); impl std::ops::Deref for Wrapper<u16> { type Target = Failure; fn deref(&self) -> &Failure { &self.0 } } impl Wrapper<u8> { fn file(&self) -> &'static str { \"\" } } fn specialized(value: &Wrapper<u16>) { let _ = value.file(); }",
        "struct Wrapper(Failure); impl std::ops::Deref for Wrapper { type Target = Failure; fn deref(&self) -> &Failure { &self.0 } } impl Wrapper { #[remove] fn file(&self) -> &'static str { \"\" } } fn rewritten_method(value: &Wrapper) { let _ = value.file(); }",
    ] {
        let original = format!("{ORIGINAL}\n{consumer}\n");
        fs::write(&path, &original).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{consumer}\n{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    }
    Ok(())
}

#[test]
fn known_consumer_blocks_only_its_candidate() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original =
        format!("{ORIGINAL}\nmod other;\nfn take(value: &Failure) -> &String {{ value.file() }}\n");
    let other = ORIGINAL.replace("Failure", "OtherFailure");
    fs::write(&path, &original).into_diagnostic()?;
    fs::write(root.path().join("src/other.rs"), &other).into_diagnostic()?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 1, "{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
        check(root.path())?;
    }
    assert_eq!(
        fs::read_to_string(root.path().join("src/other.rs")).into_diagnostic()?,
        other
    );
    Ok(())
}

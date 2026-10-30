#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;

const ORIGINAL: &str = r#"use derive_getters::Getters;
use derive_more::{Display, Error};
/// A located failure.
#[derive(Debug, Display, Error, Getters)]
#[display("failure at {file}:{line}")]
pub struct Failure {
    /// Source file.
    file: String,
    line: u32,
}
impl Failure {
    #[track_caller]
    pub fn new() -> Self {
        let loc = std::panic::Location::caller();
        Self { file: loc.file().to_string(), line: loc.line() }
    }
}
"#;

const CONDITIONAL_TUPLE: &str = r#"use derive_getters::Getters;
use derive_more::{Display, Error};
/// A located failure.
#[derive(Debug, Display, Error, Getters)]
#[display("failure at {file}:{line}")]
pub struct Failure {
    /// Source file.
    #[cfg(not(kani))]
    file: String,
    #[cfg(not(kani))]
    line: u32,
}
impl Failure {
    #[track_caller]
    pub fn new() -> Self {
        #[cfg(not(kani))]
        let (line, file) = {
            let loc = std::panic::Location::caller();
            (loc.line(), loc.file().to_string())
        };
        Self {
            #[cfg(not(kani))]
            file,
            #[cfg(not(kani))]
            line,
        }
    }
}
"#;

#[test]
fn conditional_display_getters_round_trip_in_both_configurations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(&manifest, format!("{cargo}\n[features]\nalternate = []\n")).into_diagnostic()?;
    let original = ORIGINAL.replace(
        "#[display(\"failure at {file}:{line}\")]",
        r#"#[cfg_attr(not(feature = "alternate"), display("failure at {file}:{line}"))]
#[cfg_attr(feature = "alternate", cfg_attr(all(), display("failure at {}:{}", file, line)))]"#,
    );
    let path = root.path().join("src/lib.rs");
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
        check(root.path())?;
        let output = Command::new("cargo")
            .args(["test", "--offline", "--quiet", "--features", "alternate"])
            .current_dir(root.path())
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn conditional_storage_contracts_stay_blocked_when_any_branch_is_unknown() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    for attr in [
        "#[cfg_attr(feature = \"other\", unknown)]",
        "#[cfg_attr(feature = \"other\", cfg(unix))]",
        "#[cfg_attr(feature = \"other\", display(\"{}\", file.capacity()))]",
        "#[cfg_attr(feature = \"other\", display(\"{file}\"), unknown)]",
        "#[cfg_attr(feature = \"other\", cfg_attr(unix, unknown))]",
        "#[cfg_attr(feature = \"other\", derive(unknown::Getters))]",
        "#[cfg_attr(feature = \"other\", derive(derive_getters::Getters))]",
    ] {
        let original =
            ORIGINAL.replace("pub struct Failure", &format!("{attr}\npub struct Failure"));
        fs::write(&path, &original).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{attr}\n{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    }
    Ok(())
}

#[test]
fn ignored_error_field_round_trip_preserves_derive_contract() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = ORIGINAL.replace("file: String,", "#[error(ignore)] file: String,");
    fs::write(&path, &original).into_diagnostic()?;
    check(root.path())?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 1, "{result:?}");
        assert!(
            fs::read_to_string(&path)
                .into_diagnostic()?
                .contains("#[error(ignore)]")
        );
        check(root.path())?;
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn conditional_tuple_getter_round_trip_compiles_and_restores_source() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    fs::write(&path, CONDITIONAL_TUPLE).into_diagnostic()?;
    check(root.path())?;
    for strategy in ["borrowed", "owned"] {
        configure(root.path(), strategy)?;
        let preview =
            run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
        assert_eq!(preview.converted_fields(), 1, "{preview:?}");
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 1, "{applied:?}");
        let patched = fs::read_to_string(&path).into_diagnostic()?;
        if strategy == "borrowed" {
            assert!(patched.contains("#[cfg(not(kani))]\ntype CordialFailureFileStorage"));
            assert!(patched.contains("(loc.line(), loc.file())"));
            assert!(patched.contains("#[getter(copy)]"));
        }
        check(root.path())?;
    }
    assert_eq!(
        fs::read_to_string(&path).into_diagnostic()?,
        CONDITIONAL_TUPLE
    );
    Ok(())
}

#[test]
fn complementary_tuple_origins_round_trip_in_both_configurations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let manifest = root.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(&manifest, format!("{cargo}\n[features]\nalternate = []\n")).into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    let original = complementary_tuples();
    fs::write(&path, &original).into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"#[test]
fn conditional_caller_contract() {
    let expected = line!() + 1;
    let error = getter_apply::Failure::new();
    if cfg!(feature = "alternate") {
        assert_eq!(*error.line(), 0);
        assert_eq!(error.file(), "");
        assert_eq!(error.to_string(), "failure at :0");
    } else {
        assert_eq!(*error.line(), expected);
        assert_eq!(error.file(), file!());
        assert_eq!(error.to_string(), format!("failure at {}:{expected}", file!()));
    }
}
"#,
    )
    .into_diagnostic()?;
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
        let patched = fs::read_to_string(&path).into_diagnostic()?;
        if strategy == "borrowed" {
            assert!(patched.contains("(0, \"\")"));
            assert!(patched.contains("(loc.line(), loc.file())"));
        }
        check(root.path())?;
        let output = Command::new("cargo")
            .args(["test", "--offline", "--quiet", "--features", "alternate"])
            .current_dir(root.path())
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let repeat =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(repeat.converted_fields(), 0, "{repeat:?}");
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn incomplete_or_escaped_tuple_origins_stay_unchanged() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = complementary_tuples();
    for unsupported in [
        original.replace("not(feature = \"alternate\")", "feature = \"another\""),
        original.replace("not(feature = \"alternate\")", "feature = \"alternate\""),
        original.replace("(0, String::new())", "(0, runtime_file())"),
        original.replace("(0, String::new())", "(1, String::new())"),
        original.replace(
            "Self { file, line }",
            "let _escaped = &file; Self { file, line }",
        ),
        original.replace(
            "Self { file, line }",
            "let file = String::new(); Self { file, line }",
        ),
        original.replace("let (line, file)", "let (line, mut file)"),
        original.replace(
            "#[cfg(feature = \"alternate\")]",
            "#[cfg_attr(feature = \"alternate\", cfg(unix))]",
        ),
    ] {
        fs::write(&path, &unsupported).into_diagnostic()?;
        let applied =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(applied.converted_fields(), 0, "{unsupported}\n{applied:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, unsupported);
    }
    Ok(())
}

fn complementary_tuples() -> String {
    ORIGINAL.replace(
        "let loc = std::panic::Location::caller();\n        Self { file: loc.file().to_string(), line: loc.line() }",
        r#"#[cfg(feature = "alternate")]
        let (line, file) = (0, String::new());
        #[cfg(not(feature = "alternate"))]
        let (line, file) = {
            let loc = std::panic::Location::caller();
            (loc.line(), loc.file().to_string())
        };
        Self { file, line }"#,
    )
}

#[test]
fn derived_getter_alias_round_trip_compiles_and_restores_source() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    check(root.path())?;
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
        let patched = fs::read_to_string(&path).into_diagnostic()?;
        assert!(patched.contains("Debug, Display, Error, Getters"));
        if strategy == "borrowed" {
            assert!(patched.contains("Generated by Cordial: error-field-storage v1; owner=Failure.file; getter-copy=inserted"));
            assert!(patched.contains("type CordialFailureFileStorage = &'static str;"));
            assert!(patched.contains("file: CordialFailureFileStorage"));
            assert!(patched.contains("#[getter(copy)]"));
        }
        check(root.path())?;
        let repeat =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(repeat.converted_fields(), 0, "{repeat:?}");
    }
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, ORIGINAL);
    Ok(())
}

#[test]
fn alias_repairs_close_the_worklist_and_stay_clear_on_rescan() -> miette::Result<()> {
    use cordial::{ItemState, StoreLayout, Worklist};
    cordial::init_tracing();
    let root = fixture()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(root.path());
    let store = StoreLayout::from_root(home.path().join(&slug), slug);
    run_cli(root.path(), home.path(), &[])?;
    let before = Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing list"))?;
    let fields: Vec<_> = before
        .items()
        .iter()
        .filter(|item| {
            item.rule_id().as_deref() == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001")
                && item.item_line().starts_with("`Failure::file`")
        })
        .collect();
    assert_eq!(fields.len(), 1, "{before:?}");
    let worklist = fs::read(store.root().join("worklist.json")).into_diagnostic()?;
    let source = fs::read(root.path().join("src/lib.rs")).into_diagnostic()?;
    let preview = run_cli(root.path(), home.path(), &["--apply", "--dry-run"])?;
    assert!(
        preview.contains("1 worklist item(s) would be checked off"),
        "{preview}"
    );
    assert_eq!(
        fs::read(store.root().join("worklist.json")).into_diagnostic()?,
        worklist
    );
    assert_eq!(
        fs::read(root.path().join("src/lib.rs")).into_diagnostic()?,
        source
    );
    let applied = run_cli(root.path(), home.path(), &["--apply"])?;
    assert!(
        applied.contains("1 worklist item(s) checked off"),
        "{applied}"
    );
    let after = Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing list"))?;
    for item in before.items() {
        assert_eq!(
            after
                .items()
                .iter()
                .find(|other| other.id() == item.id())
                .map(|other| other.state()),
            Some(if item.id() == fields[0].id() {
                &ItemState::Done
            } else {
                item.state()
            }),
            "{item:?}"
        );
    }
    run_cli(root.path(), home.path(), &[])?;
    let rescanned = Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing list"))?;
    assert!(
        !rescanned
            .items()
            .iter()
            .any(|item| item.rule_id().as_deref()
                == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001")
                && item.item_line().starts_with("`Failure::file`")),
        "{rescanned:?}"
    );
    Ok(())
}

#[test]
fn generated_alias_is_not_removed_when_ownership_evidence_changes() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 1, "{result:?}");
    let generated = fs::read_to_string(&path).into_diagnostic()?;
    configure(root.path(), "owned")?;
    for changed in [
        format!("{generated}\nconst EXTRA: CordialFailureFileStorage = \"extra\";\n"),
        generated.replace("getter-copy=inserted", "getter-copy=user"),
        generated.replace("&'static str;", "String;"),
        generated.replace("#[getter(copy)] ", ""),
        generated.replace("error-field-storage v1", "error-field-storage v2"),
        generated.replace("owner=Failure.file", "owner=Other.file"),
        generated.replace("#[getter(copy)]", "#[error(source)] #[getter(copy)]"),
        generated.replace("type Cordial", "pub type Cordial"),
        generated.replace(
            "/// Generated by Cordial:",
            "/// User-edited alias.\n/// Generated by Cordial:",
        ),
    ] {
        fs::write(&path, &changed).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{changed}\n{result:?}");
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, changed);
    }
    // Opaque and cross-file references are also evidence against deletion.
    for extra in [
        "const EXTRA: super::CordialFailureFileStorage = \"extra\";",
        "unknown!(CordialFailureFileStorage);",
    ] {
        fs::write(&path, format!("{generated}\nmod consumer;\n")).into_diagnostic()?;
        fs::write(root.path().join("src/consumer.rs"), extra).into_diagnostic()?;
        let before = fs::read(&path).into_diagnostic()?;
        let result =
            run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
        assert_eq!(result.converted_fields(), 0, "{result:?}");
        assert_eq!(fs::read(&path).into_diagnostic()?, before);
    }
    Ok(())
}

#[test]
fn user_aliases_are_preserved_and_names_do_not_collide() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let path = root.path().join("src/lib.rs");
    let original = format!("type CordialFailureFileStorage = &'static str;\n{ORIGINAL}");
    fs::write(&path, &original).into_diagnostic()?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 1, "{result:?}");
    let generated = fs::read_to_string(&path).into_diagnostic()?;
    assert!(generated.contains("type CordialFailureFileStorage2 = &'static str;"));
    configure(root.path(), "owned")?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.converted_fields(), 1, "{result:?}");
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

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

fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    fs::create_dir_all(root.path().join(".cargo")).into_diagnostic()?;
    fs::write(
        root.path().join(".cargo/config.toml"),
        "[net]\noffline = true\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        r#"[package]
name = "getter-apply"
version = "0.0.0"
edition = "2024"
[dependencies]
derive-getters = "=0.5.0"
derive_more = { version = "2", features = ["display", "error"] }
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(kani)"] }
"#,
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), ORIGINAL).into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"#[test]
fn local_error_getters_preserve_caller() {
    let expected = line!() + 1;
    let error = getter_apply::Failure::new();
    assert_eq!(*error.line(), expected);
    assert_eq!(error.file(), file!());
    assert_eq!(error.to_string(), format!("failure at {}:{expected}", file!()));
}
"#,
    )
    .into_diagnostic()?;
    configure(root.path(), "borrowed")?;
    Ok(root)
}

fn configure(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
}

fn check(root: &Path) -> miette::Result<()> {
    let output = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn run_cli(root: &Path, home: &Path, args: &[&str]) -> miette::Result<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(["quality", "--etiquette", "internal_error_chain"])
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

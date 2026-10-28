use std::fs;
use std::path::Path;
use std::process::Command;

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;

#[cfg(all(feature = "cli", feature = "quality"))]
#[test]
fn distributed_apply_claims_exact_findings_in_both_strategies() -> miette::Result<()> {
    cordial::init_tracing();
    use cordial::{ItemState, Worklist};

    let root = distributed_fixture()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(root.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    let run = |apply: bool| -> miette::Result<String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
        command
            .arg("--project")
            .arg(root.path())
            .arg("--store-home")
            .arg(home.path())
            .args(["quality", "--etiquette", "internal_error_chain"]);
        if apply {
            command.arg("--apply");
        }
        let result = command
            .env("CORDIAL_NO_PROGRESS", "1")
            .output()
            .into_diagnostic()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    };
    for (strategy, expected) in [("borrowed", 5), ("owned", 1)] {
        fs::write(
            root.path().join("cordial.toml"),
            format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
        )
        .into_diagnostic()?;
        run(false)?;
        let before = Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing worklist"))?;
        let matching: Vec<_> = before
            .items()
            .iter()
            .filter(|item| {
                item.rule_id().as_deref() == Some("ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001")
                    || (strategy == "borrowed"
                        && item.rule_id().as_deref()
                            == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001"))
            })
            .collect();
        assert_eq!(matching.len(), expected, "{matching:?}");
        let output = run(true)?;
        assert!(
            output.contains(&format!("{expected} worklist item(s) checked off")),
            "{strategy}: {output}"
        );
        let after = Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing updated worklist"))?;
        for original in before.items() {
            let claimed = matching.iter().any(|item| item.id() == original.id());
            let item = after.items().iter().find(|item| item.id() == original.id());
            assert_eq!(
                item.map(|item| item.state()),
                Some(if claimed {
                    &ItemState::Done
                } else {
                    original.state()
                }),
                "{strategy}: {}",
                original.item_line()
            );
        }
    }
    Ok(())
}

#[test]
fn private_error_graph_across_files_round_trips() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    check(root.path())?;

    let preview = run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.changed_files(), 2, "{:?}", preview.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(
        fs::read_to_string(&child).into_diagnostic()?,
        original_child
    );

    let borrowed = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 2, "{:?}", borrowed.blockers());
    let borrowed_library = fs::read_to_string(&library).into_diagnostic()?;
    let borrowed_child = fs::read_to_string(&child).into_diagnostic()?;
    assert!(borrowed_library.contains("source: &'a Cause"));
    assert!(borrowed_child.contains("source: source,"));
    check(root.path())?;

    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"owned\"\n",
    )
    .into_diagnostic()?;
    let owned = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 2, "{:?}", owned.blockers());
    check(root.path())?;
    Ok(())
}

#[test]
fn unresolved_external_field_read_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nfn read(error: &super::Parent<'_>) -> &super::Cause {{ &error.source }}\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn hidden_external_graph_name_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(
        &child,
        format!("{original_child}\nmacro_rules! hidden {{ () => {{ super::Parent }}; }}\n"),
    )
    .into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn same_named_external_declaration_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nstruct Parent;\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn distributed_kind_and_source_declarations_round_trip() -> miette::Result<()> {
    cordial::init_tracing();
    let root = distributed_fixture()?;
    check(root.path())?;
    let original: Vec<_> = ["lib.rs", "cause.rs", "kind.rs", "build.rs"]
        .into_iter()
        .map(|name| fs::read_to_string(root.path().join("src").join(name)))
        .collect::<Result<_, _>>()
        .into_diagnostic()?;

    let preview = run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.changed_files(), 4, "{:?}", preview.blockers());
    for (name, content) in ["lib.rs", "cause.rs", "kind.rs", "build.rs"]
        .into_iter()
        .zip(&original)
    {
        assert_eq!(
            fs::read_to_string(root.path().join("src").join(name)).into_diagnostic()?,
            *content
        );
    }

    let borrowed = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 4, "{:?}", borrowed.blockers());
    let library = fs::read_to_string(root.path().join("src/lib.rs")).into_diagnostic()?;
    let cause = fs::read_to_string(root.path().join("src/cause.rs")).into_diagnostic()?;
    let kind = fs::read_to_string(root.path().join("src/kind.rs")).into_diagnostic()?;
    assert!(library.contains("source: &'a cause::Cause"), "{library}");
    assert!(library.contains("kind: &'a kind::Kind<'a>"), "{library}");
    assert!(cause.contains("file: &'static str"), "{cause}");
    assert!(kind.contains("Wrapped(&'a super::cause::Cause)"), "{kind}");
    check(root.path())?;

    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"owned\"\n",
    )
    .into_diagnostic()?;
    let owned = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 4, "{:?}", owned.blockers());
    check(root.path())?;
    Ok(())
}

#[test]
fn hidden_use_blocks_all_distributed_declarations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = distributed_fixture()?;
    let files = ["lib.rs", "cause.rs", "kind.rs", "build.rs"];
    let build = root.path().join("src/build.rs");
    let original_build = fs::read_to_string(&build).into_diagnostic()?;
    fs::write(
        &build,
        format!("{original_build}\nmacro_rules! hidden {{ () => {{ super::Parent }}; }}\n"),
    )
    .into_diagnostic()?;
    let before: Vec<_> = files
        .iter()
        .map(|name| fs::read_to_string(root.path().join("src").join(name)))
        .collect::<Result<_, _>>()
        .into_diagnostic()?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0, "{:?}", result.blockers());
    for (name, content) in files.into_iter().zip(before) {
        assert_eq!(
            fs::read_to_string(root.path().join("src").join(name)).into_diagnostic()?,
            content
        );
    }
    Ok(())
}

#[test]
fn distributed_graph_introduces_and_removes_one_lifetime() -> miette::Result<()> {
    cordial::init_tracing();
    let root = lifetime_free_distributed_fixture()?;
    let names = ["lib.rs", "cause.rs", "kind.rs", "build.rs"];
    check(root.path())?;
    let original: Vec<_> = names
        .iter()
        .map(|name| fs::read_to_string(root.path().join("src").join(name)))
        .collect::<Result<_, _>>()
        .into_diagnostic()?;

    let preview = run_error_file_apply(root.path(), root.path(), None, true).into_diagnostic()?;
    assert_eq!(preview.changed_files(), 4, "{:?}", preview.blockers());
    for (name, before) in names.iter().zip(&original) {
        assert_eq!(
            fs::read_to_string(root.path().join("src").join(name)).into_diagnostic()?,
            *before
        );
    }

    let borrowed = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(borrowed.changed_files(), 4, "{:?}", borrowed.blockers());
    let library = fs::read_to_string(root.path().join("src/lib.rs")).into_diagnostic()?;
    let kind = fs::read_to_string(root.path().join("src/kind.rs")).into_diagnostic()?;
    let build = fs::read_to_string(root.path().join("src/build.rs")).into_diagnostic()?;
    assert!(library.contains("struct Parent<'a>"), "{library}");
    assert!(kind.contains("enum Kind<'a>"), "{kind}");
    assert!(build.contains("super::Parent<'a>"), "{build}");
    check(root.path())?;
    assert_eq!(
        run_error_file_apply(root.path(), root.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );

    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"owned\"\n",
    )
    .into_diagnostic()?;
    let owned = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(owned.changed_files(), 4, "{:?}", owned.blockers());
    check(root.path())?;
    assert_eq!(
        run_error_file_apply(root.path(), root.path(), None, false)
            .into_diagnostic()?
            .changed_files(),
        0
    );
    Ok(())
}

#[test]
fn distributed_lifetime_rejects_unanchored_constructor() -> miette::Result<()> {
    cordial::init_tracing();
    let root = lifetime_free_distributed_fixture()?;
    let path = root.path().join("src/lib.rs");
    let source = fs::read_to_string(&path).into_diagnostic()?;
    fs::write(
        &path,
        source.replace(
            "let kind = kind::make(&cause);",
            "let kind = kind::Kind::Wrapped(Box::new(cause.clone()));",
        ),
    )
    .into_diagnostic()?;
    check(root.path())?;
    let names = ["lib.rs", "cause.rs", "kind.rs", "build.rs"];
    let before: Vec<_> = names
        .iter()
        .map(|name| fs::read_to_string(root.path().join("src").join(name)))
        .collect::<Result<_, _>>()
        .into_diagnostic()?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0, "{:?}", result.blockers());
    for (name, expected) in names.iter().zip(before) {
        assert_eq!(
            fs::read_to_string(root.path().join("src").join(name)).into_diagnostic()?,
            expected
        );
    }
    Ok(())
}

fn lifetime_free_distributed_fixture() -> miette::Result<tempfile::TempDir> {
    let root = distributed_fixture()?;
    fs::write(root.path().join("src/lib.rs"), r#"mod cause;
mod kind;
mod build;
use std::panic::Location;

#[derive(Debug, Clone)]
struct Parent { source: cause::Cause, kind: Box<kind::Kind>, file: String }
impl Parent {
    #[track_caller]
    fn new<'a>(source: &'a cause::Cause, kind: &'a kind::Kind) -> Self {
        Self { source: source.clone(), kind: Box::new(kind.clone()), file: Location::caller().file().to_string() }
    }
}
impl std::fmt::Display for Parent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
impl<'a> From<(&'a cause::Cause, &'a kind::Kind)> for Parent {
    fn from((source, kind): (&'a cause::Cause, &'a kind::Kind)) -> Self { Self::new(source, kind) }
}
#[test]
fn preserves_source_and_location() {
    use std::error::Error;
    let cause = cause::Cause::new();
    let kind = kind::make(&cause);
    let error = build::via_try(&cause, &kind).unwrap_err();
    assert!(error.source().unwrap().to_string().contains("lib.rs"));
    assert!(error.to_string().contains("lib.rs"));
}
"#).into_diagnostic()?;
    fs::write(
        root.path().join("src/kind.rs"),
        r#"#[derive(Debug, Clone)]
pub(super) enum Kind { Wrapped(Box<super::cause::Cause>) }
pub(super) fn make<'a>(source: &'a super::cause::Cause) -> Kind {
    Kind::Wrapped(Box::new(source.clone()))
}
"#,
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/build.rs"), r#"pub(super) fn via_try<'a>(source: &'a super::cause::Cause, kind: &'a super::kind::Kind) -> Result<(), super::Parent> {
    let input: Result<(), (&super::cause::Cause, &super::kind::Kind)> = Err((source, kind));
    input?;
    Ok(())
}
"#).into_diagnostic()?;
    Ok(root)
}

fn distributed_fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-distributed-error-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"mod cause;
mod kind;
mod build;

#[derive(Debug, Clone)]
struct Parent<'a> {
    source: cause::Cause,
    kind: Box<kind::Kind<'a>>,
    file: String,
    anchor: &'a (),
}
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/cause.rs"),
        r#"use std::panic::Location;
#[derive(Debug, Clone)]
pub(super) struct Cause { file: String }
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Cause {}
impl Cause {
    #[track_caller]
    pub(super) fn new() -> Self { Self { file: Location::caller().file().to_string() } }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/kind.rs"),
        r#"#[derive(Debug, Clone)]
pub(super) enum Kind<'a> {
    Wrapped(Box<super::cause::Cause>),
    Marker(&'a ()),
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/build.rs"),
        r#"use std::panic::Location;

pub(super) fn kind<'a>(source: &'a super::cause::Cause, anchor: &'a ()) -> super::kind::Kind<'a> {
    let _ = anchor;
    super::kind::Kind::Wrapped(Box::new(source.clone()))
}

#[track_caller]
pub(super) fn parent<'a>(
    source: &'a super::cause::Cause,
    kind: &'a super::kind::Kind<'a>,
    anchor: &'a (),
) -> super::Parent<'a> {
    super::Parent {
        source: source.clone(),
        kind: Box::new(kind.clone()),
        file: Location::caller().file().to_string(),
        anchor,
    }
}
"#,
    )
    .into_diagnostic()?;
    Ok(root)
}

fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-cross-file-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/lib.rs"),
        r#"mod child;
#[derive(Debug, Clone)]
struct Cause;
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cause") }
}
impl std::error::Error for Cause {}

#[derive(Debug, Clone)]
struct Parent<'a> { source: Cause, file: String, anchor: &'a () }
impl std::fmt::Display for Parent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.file) }
}
impl std::error::Error for Parent<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.source) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_source() {
        use std::error::Error;
        let cause = super::Cause;
        let anchor = ();
        let error = super::child::make(&cause, &anchor);
        assert_eq!(error.source().unwrap().to_string(), "cause");
    }
}
"#,
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("src/child.rs"),
        r#"use std::panic::Location;
#[track_caller]
pub(super) fn make<'a>(source: &'a super::Cause, anchor: &'a ()) -> super::Parent<'a> {
    super::Parent { source: source.clone(), file: Location::caller().file().to_string(), anchor }
}
"#,
    )
    .into_diagnostic()?;
    Ok(root)
}

fn check(root: &Path) -> miette::Result<()> {
    let output = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    if !output.status.success() {
        return Err(miette::miette!(
            "fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

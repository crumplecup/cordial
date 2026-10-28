use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use cordial::{ItemState, StoreLayout, Worklist};
use miette::IntoDiagnostic;

const SOURCE: &str = r#"#[derive(Debug)]
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
}
"#;

#[test]
fn apply_claims_only_repaired_fields_and_dry_run_preserves_worklist() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), store_home.path());
    run(
        root.path(),
        store_home.path(),
        &["quality", "--etiquette", "internal_error_chain"],
    )?;
    let before = list(&store)?;
    let fields: Vec<_> = before
        .items()
        .iter()
        .filter(|item| {
            item.rule_id().as_deref() == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001")
        })
        .collect();
    assert_eq!(fields.len(), 2, "{fields:?}");
    assert_eq!(fields[0].line(), fields[1].line());
    let worklist_bytes = fs::read(store.root().join("worklist.json")).into_diagnostic()?;
    let apply = ["quality", "--apply", "--etiquette", "internal_error_chain"];
    let mut preview = apply.to_vec();
    preview.push("--dry-run");
    let dry = run(root.path(), store_home.path(), &preview)?;
    let output = String::from_utf8_lossy(&dry.stdout);
    assert!(output.contains("1 field(s) would be converted"), "{output}");
    assert!(output.contains("1 candidate field(s) blocked"), "{output}");
    assert!(
        output.contains("MessageError::message — Initializer"),
        "{output}"
    );
    assert!(
        output.contains("0 worklist item(s) checked off"),
        "{output}"
    );
    assert!(
        output.contains("1 worklist item(s) would be checked off"),
        "{output}"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/lib.rs")).into_diagnostic()?,
        SOURCE
    );
    assert_eq!(
        fs::read(store.root().join("worklist.json")).into_diagnostic()?,
        worklist_bytes
    );

    let applied = run(root.path(), store_home.path(), &apply)?;
    let after = list(&store)?;
    for item in before.items() {
        let repaired = item.rule_id().as_deref()
            == Some("ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001")
            && item.item_line().starts_with("`MessageError::context`");
        let current = after
            .items()
            .iter()
            .find(|current| current.id() == item.id());
        assert_eq!(
            current.map(|item| item.state()),
            Some(if repaired {
                &ItemState::Done
            } else {
                item.state()
            }),
            "{}",
            item.item_line()
        );
    }
    let remaining = after
        .items()
        .iter()
        .filter(|item| *item.state() == ItemState::Open)
        .count();
    let output = String::from_utf8_lossy(&applied.stdout);
    assert!(
        output.contains("1 worklist item(s) checked off"),
        "{output}"
    );
    assert!(
        output.contains(&format!("{remaining} error handling item(s) remain open")),
        "{output}"
    );
    let repeated = run(root.path(), store_home.path(), &apply)?;
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("0 worklist item(s) checked off"));
    assert_eq!(list(&store)?, after);
    Ok(())
}

#[test]
fn apply_does_not_create_or_claim_an_outdated_worklist() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), store_home.path());
    let apply = ["quality", "--apply", "--etiquette", "internal_error_chain"];
    let missing = run(root.path(), store_home.path(), &apply)?;
    assert!(String::from_utf8_lossy(&missing.stdout).contains("no current worklist"));
    assert_eq!(Worklist::read(&store).into_diagnostic()?, None);
    fs::write(root.path().join("src/lib.rs"), SOURCE).into_diagnostic()?;
    run(
        root.path(),
        store_home.path(),
        &["quality", "--etiquette", "internal_error_chain"],
    )?;
    let before = list(&store)?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\nkind_fanout_max_call_sites = 999\n",
    )
    .into_diagnostic()?;
    let outdated = run(root.path(), store_home.path(), &apply)?;
    assert!(String::from_utf8_lossy(&outdated.stdout).contains("worklist is out of date"));
    assert_eq!(list(&store)?, before);
    assert!(
        fs::read_to_string(root.path().join("src/lib.rs"))
            .into_diagnostic()?
            .contains("context: &'static str")
    );
    Ok(())
}

fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"apply-worklist-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), SOURCE).into_diagnostic()?;
    Ok(root)
}

fn store(project: &Path, home: &Path) -> StoreLayout {
    let slug = cordial::project_slug_from_path(project);
    StoreLayout::from_root(home.join(&slug), slug)
}

fn list(store: &StoreLayout) -> miette::Result<Worklist> {
    Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing worklist"))
}

fn run(project: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    let result = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()?;
    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result)
}

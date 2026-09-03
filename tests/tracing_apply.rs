use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};

use cordial::{parse_tracing_instrument_checklist_text, run_tracing_instrument_apply};

struct ApplyFixture {
    workspace: PathBuf,
    src: PathBuf,
    checklist: PathBuf,
    _temp: tempfile::TempDir,
}

fn write_apply_fixture(source: &str, checklist: &str) -> miette::Result<ApplyFixture> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    let crate_root = workspace.join("fixture_crate");
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(src_root.join("lib.rs"), source)
        .into_diagnostic()
        .wrap_err("write source")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"fixture_crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()
    .wrap_err("crate manifest")?;
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"fixture_crate\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    let checklist_path = temp.path().join("tracing-instrument.checklist.md");
    fs::write(&checklist_path, checklist)
        .into_diagnostic()
        .wrap_err("write checklist")?;
    Ok(ApplyFixture {
        workspace,
        src: src_root.join("lib.rs"),
        checklist: checklist_path,
        _temp: temp,
    })
}

fn checklist_for(items: &[(&str, u32)]) -> String {
    checklist_for_crate("fixture_crate", items)
}

fn checklist_for_crate(crate_name: &str, items: &[(&str, u32)]) -> String {
    let mut body =
        format!("# Tracing instrument checklist\n\n**Open gaps:** n\n\n## `{crate_name}`\n\n");
    for (qualified_name, line) in items {
        body.push_str(&format!(
            "- [ ] `{qualified_name}` — `src/lib.rs:{line}` (pub)\n"
        ));
    }
    body
}

/// A workspace with one or more crates, its own `cordial.toml`, and a
/// checklist -- for verifier-policy tests, where a single synthetic
/// crate (as [`write_apply_fixture`] builds) isn't enough to exercise
/// dependency-graph or `#[path]`-splice propagation.
struct PolicyFixture {
    workspace: PathBuf,
    checklist: PathBuf,
    _temp: tempfile::TempDir,
}

/// Write one workspace member crate. `deps` are `(dependency crate
/// name, relative path to it from this crate's own directory)` pairs,
/// written as real Cargo path dependencies.
fn write_policy_crate(
    workspace: &Path,
    name: &str,
    source: &str,
    deps: &[(&str, &str)],
) -> miette::Result<()> {
    let crate_root = workspace.join(name);
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(src_root.join("lib.rs"), source)
        .into_diagnostic()
        .wrap_err("write source")?;
    let mut manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
    if !deps.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (dep_name, dep_rel_path) in deps {
            manifest.push_str(&format!("{dep_name} = {{ path = \"{dep_rel_path}\" }}\n"));
        }
    }
    fs::write(crate_root.join("Cargo.toml"), manifest)
        .into_diagnostic()
        .wrap_err("crate manifest")?;
    Ok(())
}

fn write_policy_fixture(
    members: &[&str],
    cordial_toml: &str,
    checklist: &str,
) -> miette::Result<PolicyFixture> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace)
        .into_diagnostic()
        .wrap_err("workspace dir")?;
    let members_list = members
        .iter()
        .map(|member| format!("\"{member}\""))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        workspace.join("Cargo.toml"),
        format!("[workspace]\nmembers = [{members_list}]\nresolver = \"2\"\n"),
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    fs::write(workspace.join("cordial.toml"), cordial_toml)
        .into_diagnostic()
        .wrap_err("cordial.toml")?;
    let checklist_path = temp.path().join("tracing-instrument.checklist.md");
    fs::write(&checklist_path, checklist)
        .into_diagnostic()
        .wrap_err("write checklist")?;
    Ok(PolicyFixture {
        workspace,
        checklist: checklist_path,
        _temp: temp,
    })
}

#[test]
fn parse_tracing_checklist_groups_by_crate_section() -> miette::Result<()> {
    cordial::init_tracing();
    let body = fs::read_to_string("tests/fixtures/quality/apply_checklist.md")
        .into_diagnostic()
        .wrap_err("read apply checklist fixture")?;
    let gaps = parse_tracing_instrument_checklist_text(&body)
        .into_diagnostic()
        .wrap_err("parse checklist")?;
    assert_eq!(gaps.len(), 2);
    assert_eq!(gaps[0].crate_name(), "fixture_crate");
    assert_eq!(gaps[0].qualified_name(), "apply_target::missing");
    assert_eq!(gaps[0].rel_path(), Path::new("src/lib.rs"));
    assert_eq!(gaps[0].line(), 1);
    Ok(())
}

#[test]
fn apply_inserts_instrument_from_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let source = fs::read_to_string("tests/fixtures/quality/apply_target/src/lib.rs")
        .into_diagnostic()
        .wrap_err("read apply target")?;
    let checklist = fs::read_to_string("tests/fixtures/quality/apply_checklist.md")
        .into_diagnostic()
        .wrap_err("read apply checklist")?;
    let fixture = write_apply_fixture(&source, &checklist)?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    assert_eq!(summary.changed_functions, 2);
    assert_eq!(summary.changed_files, 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(updated.contains("#[instrument(level = \"debug\", skip(path, report))]"));
    assert!(updated.contains("#[instrument(level = \"debug\", skip(path))]"));
    assert!(updated.contains("use tracing::instrument;"));
    Ok(())
}

#[test]
fn apply_writes_getter_trace_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Store {
    root: String,
}

impl Store {
    pub fn as_str(&self) -> &str {
        &self.root
    }
}
"#,
        &checklist_for(&[("Store::as_str", 7)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"trace\", skip(self))]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_writes_io_err_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 2)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"info\", err(level = \"warn\"))]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_writes_entry_recipe_for_run_prefix() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn run_apply_patches() {}
"#,
        &checklist_for(&[("run_apply_patches", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"info\")]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_does_not_treat_free_path_fn_as_getter() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn trait_impls_for_path(key: &str) -> Option<u8> {
    None
}
"#,
        &checklist_for(&[("trait_impls_for_path", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"debug\")]"),
        "free *_path is Other at debug, not a trace getter: {updated}"
    );
    assert!(!updated.contains("level = \"trace\""), "{updated}");
    Ok(())
}

#[test]
fn apply_writes_err_for_result_alias() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub type CordialResult<T> = Result<T, String>;

pub fn load_session_config() -> CordialResult<()> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"info\", err(level = \"warn\"))]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_rewrites_existing_instrument_to_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[tracing::instrument]
    pub fn cache_dir(&self) -> String {
        format!("{}/cache", self.root)
    }
}
"#,
        &checklist_for(&[("Store::cache_dir", 8)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);
    assert_eq!(summary.skipped_existing, 0);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"trace\", skip(self))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("#[tracing::instrument]"),
        "bare default-info attr should be replaced: {updated}"
    );
    assert!(updated.contains("use tracing::instrument;"));
    Ok(())
}

#[test]
fn apply_skips_when_recipe_already_present() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
use tracing::instrument;

#[instrument(level = "info", err(level = "warn"))]
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 5)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 0);
    assert_eq!(summary.skipped_existing, 1);
    Ok(())
}

#[test]
fn apply_dedupes_multiple_checklist_rows_for_one_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let mut checklist = checklist_for(&[("load_session_config", 2)]);
    checklist.push_str("- [ ] `load_session_config` — `src/lib.rs:2` (pub)\n");
    let fixture = write_apply_fixture(
        r#"
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist,
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    let count = updated.matches("#[instrument(").count();
    assert_eq!(count, 1, "{updated}");
    Ok(())
}

#[test]
fn apply_does_not_split_doc_from_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
/// A documented helper.
pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 3)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("/// A documented helper.\nuse tracing::instrument;"),
        "use must not land between docs and the item:\n{updated}"
    );
    let use_idx = updated
        .find("use tracing::instrument;")
        .ok_or_else(|| miette::miette!("expected `use tracing::instrument;` in:\n{updated}"))?;
    let doc_idx = updated
        .find("/// A documented helper.")
        .ok_or_else(|| miette::miette!("expected doc comment in:\n{updated}"))?;
    assert!(use_idx < doc_idx, "{updated}");
    Ok(())
}

#[test]
fn apply_does_not_put_err_on_option() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn scan_first() -> Option<u8> {
    let value = std::fs::read_to_string("x").ok()?;
    value.bytes().next()
}
"#,
        &checklist_for(&[("scan_first", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("err("), "{updated}");
    Ok(())
}

#[test]
fn apply_matches_fn_name_not_nearest_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn fmt() {}

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("pub fn fmt() {}"),
        "stale checklist line must not stamp the nearer fn:\n{updated}"
    );
    assert!(
        updated.contains("#[instrument(level = \"debug\")]\npub fn scan_tree()"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_uses_crate_path_when_tracing_is_a_module() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
mod tracing;

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[::tracing::instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("use tracing::instrument;"), "{updated}");
    Ok(())
}

#[test]
fn apply_does_not_split_derive_from_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
#[derive(Debug)]
pub enum Boom {
    A,
}

pub fn load_config() -> Result<(), Boom> {
    Ok(())
}
"#,
        &checklist_for(&[("load_config", 7)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(updated.contains("use tracing::instrument;"), "{updated}");
    assert!(
        !updated.contains("#[derive(Debug)]\nuse tracing::instrument;"),
        "use must not land between derive and the item:\n{updated}"
    );
    let use_idx = updated
        .find("use tracing::instrument;")
        .ok_or_else(|| miette::miette!("expected `use tracing::instrument;` in:\n{updated}"))?;
    let derive_idx = updated
        .find("#[derive(Debug)]")
        .ok_or_else(|| miette::miette!("expected `#[derive(Debug)]` in:\n{updated}"))?;
    assert!(use_idx < derive_idx, "{updated}");
    Ok(())
}

#[test]
fn apply_skips_impl_trait_params() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Artifact;

impl Artifact {
    pub fn new(crate_name: impl Into<String>) -> Self {
        let _ = crate_name.into();
        Self
    }
}
"#,
        &checklist_for(&[("Artifact::new", 6)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"debug\", skip(crate_name))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("fields(crate_name"),
        "impl Trait identity args are unrecordable: {updated}"
    );
    Ok(())
}

#[test]
fn apply_does_not_duplicate_grouped_tracing_import() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
use tracing::{debug, instrument};

pub fn scan_tree() {
    debug!("scan");
}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated.matches("use tracing::").count(), 1, "{updated}");
    assert!(
        updated.contains("#[instrument(level = \"debug\")]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_uses_path_form_when_instrument_is_a_module() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
mod instrument;

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[tracing::instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("use tracing::instrument;"), "{updated}");
    Ok(())
}

#[test]
fn apply_gates_instrument_for_configured_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 2)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);
    assert_eq!(summary.skipped_policy, 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("use tracing::instrument;"),
        "a gated attribute is always fully qualified, so the plain import \
         would go unused whenever every applied gap in the file is gated \
         -- real precedent: amenable_kani has Kani-proof-only files where \
         this exact import went `unused_imports`-flagged: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_configured_crate_leaving_checklist_open() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 2)]),
    )?;
    let source = "\npub fn scan_tree() {}\n";
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 0);
    assert_eq!(summary.changed_files, 0);
    assert_eq!(summary.skipped_policy, 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "skipped file must be left byte-identical");
    Ok(())
}

#[test]
fn apply_gates_dependency_crate_via_transitive_dependent() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_core", "fixture_kani"],
        "[tracing]\napply_gate_crates = { fixture_kani = \"kani\" }\n",
        &checklist_for_crate("fixture_core", &[("scan_tree", 2)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_core",
        "\npub fn scan_tree() {}\n",
        &[],
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_kani",
        "\npub fn noop() {}\n",
        &[("fixture_core", "../fixture_core")],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_core/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "fixture_core compiles as part of fixture_kani's own `cargo kani` build, so it needs \
         the same gate even though nothing in fixture_core's own config names it: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_file_spliced_into_skip_configured_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_owner", "fixture_verus"],
        "[tracing]\napply_skip_crates = [\"fixture_verus\"]\n",
        &checklist_for_crate("fixture_owner", &[("scan_tree", 2)]),
    )?;
    let owner_source = "\npub fn scan_tree() {}\n";
    write_policy_crate(&fixture.workspace, "fixture_owner", owner_source, &[])?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_verus",
        "#[path = \"../../fixture_owner/src/lib.rs\"]\nmod owner_impl;\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 0);
    assert_eq!(summary.skipped_policy, 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_owner/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(
        updated, owner_source,
        "fixture_verus splices this exact file in via #[path], so it must stay untouched \
         even though fixture_owner itself isn't in apply_skip_crates: {updated}"
    );
    Ok(())
}

#[test]
fn apply_skips_function_nested_in_ancestor_gate_cfg() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("proof_harness", 4)]),
    )?;
    let source = r#"
#[cfg(kani)]
mod proofs {
    pub fn proof_harness() {}
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions, 0,
        "proof_harness only exists at all under `#[cfg(kani)]`, and Gated \
         policy already suppresses `#[instrument]` whenever `kani` *is* \
         active -- so #[instrument] can never fire in any real build; the \
         checklist row can't be resolved to a recorded function at all \
         (the scanner never records it in the first place), not merely \
         applied with a qualified path"
    );
    assert_eq!(summary.unresolved, 1);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}

#[test]
fn apply_skips_function_called_only_from_proof_context() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Ensures::ensures", 8)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions, 0,
        "Checker::ensures's only known caller is proofs::harness, itself \
         nested in #[cfg(kani)] -- apply must not write a span (gated or \
         otherwise); with no #[instrument] to strip, the file stays put"
    );
    assert_eq!(summary.skipped_policy, 1);
    assert_eq!(summary.unresolved, 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}

#[test]
fn apply_finds_calls_inside_a_harness_style_macro_invocation() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Ensures::ensures", 6)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

fake_harness_macro::harness! {
    kani, VERIFY_CHECKER_SRC, {
        #[kani::proof]
        fn verify_checker() {
            assert!(Checker::ensures(1));
        }
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(
        summary.changed_functions, 0,
        "syn never expands macros -- `fake_harness_macro::harness! {{ .. }}` \
         parses as an opaque Item::Macro, so without extracting its \
         trailing brace-block of real items, the call graph would never \
         see `#[kani::proof] fn verify_checker` at all, let alone the \
         `Checker::ensures` call inside it; the real `amenable_derive::\
         harness!` macro this is modeled on wraps almost every Kani proof \
         harness in amenable_kani the same way. Apply must not write a \
         span; with none to strip, the file stays put"
    );
    assert_eq!(summary.skipped_policy, 1);
    assert_eq!(summary.unresolved, 0);

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated, source, "{updated}");
    Ok(())
}

#[test]
fn apply_strips_instrument_from_proof_only_method() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Ensures::ensures", 9)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[tracing::instrument(level = "trace")]
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1, "{summary:?}");
    assert_eq!(summary.unresolved, 0, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument"),
        "proof-only Ensures::ensures must lose #[instrument], not keep a \
         not(kani) wrap that never fires: {updated}"
    );
    assert!(updated.contains("fn ensures(input: u32)"), "{updated}");
    Ok(())
}

#[test]
fn apply_strips_gated_instrument_from_proof_only_method() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Ensures::ensures", 9)]),
    )?;
    let source = r#"
pub trait Ensures<T> {
    fn ensures(input: T) -> bool;
}

pub struct Checker;

impl Ensures<u32> for Checker {
    #[cfg_attr(not(kani), tracing::instrument(level = "trace"))]
    fn ensures(input: u32) -> bool {
        input > 0
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Checker, Ensures};

    pub fn harness() {
        assert!(Checker::ensures(1));
    }
}
"#;
    write_policy_crate(&fixture.workspace, "fixture_crate", source, &[])?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument") && !updated.contains("cfg_attr"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_strips_instrument_from_skip_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_skip_crates = [\"fixture_crate\"]\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 3)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\n#[tracing::instrument(level = \"debug\")]\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1, "{summary:?}");
    assert_eq!(summary.skipped_policy, 0, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("instrument"),
        "skip-policy apply must remove existing #[instrument]: {updated}"
    );
    assert!(updated.contains("pub fn scan_tree()"), "{updated}");
    Ok(())
}

#[test]
fn apply_gates_bare_instrument_on_ordinary_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("scan_tree", 3)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        "\n#[tracing::instrument(level = \"debug\")]\npub fn scan_tree() {}\n",
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[cfg_attr(not(kani), tracing::instrument(level = \"debug\"))]"),
        "{updated}"
    );
    Ok(())
}

/// Real bug (found applying to `amenable_kani::net_model`, 2026-08-27):
/// an existing attribute that rustfmt has wrapped across multiple physical
/// lines -- `#[cfg_attr(\n  not(kani),\n  tracing::instrument(...)\n)]`,
/// the shape a long gated recipe line commonly takes -- was invisible to
/// `collect_attr_indices`'s single-line-only scan, so a mismatch never
/// replaced it: a second, corrected attribute was inserted right below the
/// untouched original, leaving the function double-instrumented.
#[test]
fn apply_rewrites_a_multi_line_gated_attribute_without_duplicating_it() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Store::write_payload", 6)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[cfg_attr(
        not(kani),
        tracing::instrument(level = "warn", skip(self, target, payload))
    )]
    pub fn write_payload(&self, target: &str, payload: Vec<u8>) -> String {
        format!("{}/{}/{}", self.root, target, payload.len())
    }
}
"#,
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(
        updated.matches("cfg_attr").count(),
        1,
        "the stale multi-line attribute must be replaced, not left behind \
         alongside a second corrected one: {updated}"
    );
    assert!(
        !updated.contains("level = \"warn\""),
        "the mismatched level must actually be rewritten: {updated}"
    );
    Ok(())
}

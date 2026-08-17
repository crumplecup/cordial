//! Compare cordial findings artifacts against frozen elicit_doc baselines.

#[cfg(feature = "impl_coverage")]
mod minimal_fixture;
#[cfg(feature = "shadow")]
mod shadow_fixture;

#[cfg(feature = "impl_coverage")]
pub use minimal_fixture::{
    IMPL_GAPS_KEY_COLUMNS, filter_impl_gaps_by_crate, impl_gaps_open, normalize_elicit_impl_gaps,
    run_cordial_impl_coverage, seed_minimal_impl_fixture, write_minimal_rustdoc,
    write_minimal_rustdoc_file,
};
#[cfg(feature = "shadow")]
pub use shadow_fixture::{
    SHADOW_GAPS_KEY_COLUMNS, SHADOW_PAIR_KEY_COLUMNS, filter_shadow_gaps_by_target,
    filter_shadow_pair_by_item_path, run_cordial_shadow_coverage, seed_minimal_shadow_fixture,
    seed_shadow_dep_rustdoc, shadow_gaps_open,
};

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use cordial::{Etiquette, RunAll, Session, SessionBuilder, quality_etiquettes};
use miette::{IntoDiagnostic, WrapErr};

/// Parsed CSV with header row.
#[derive(Debug, Clone)]
pub struct CsvTable {
    pub rows: Vec<HashMap<String, String>>,
}

impl CsvTable {
    pub fn parse(content: &str) -> miette::Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(content.as_bytes());
        let headers: Vec<String> = reader
            .headers()
            .into_diagnostic()
            .wrap_err("csv headers")?
            .iter()
            .map(str::to_string)
            .collect();
        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.into_diagnostic().wrap_err("csv record")?;
            rows.push(
                headers
                    .iter()
                    .zip(record.iter())
                    .map(|(header, value)| (header.clone(), value.to_string()))
                    .collect(),
            );
        }
        Ok(Self { rows })
    }

    pub fn open_rows<F>(&self, is_open: F) -> Vec<&HashMap<String, String>>
    where
        F: Fn(&HashMap<String, String>) -> bool,
    {
        self.rows.iter().filter(|row| is_open(row)).collect()
    }
}

/// Normalize file paths to `src/...` for cross-tool comparison.
pub fn normalize_file_path(path: &str) -> String {
    if let Some(idx) = path.rfind("src/") {
        path[idx..].replace('\\', "/")
    } else {
        path.replace('\\', "/")
    }
}

fn row_key(row: &HashMap<String, String>, columns: &[&str]) -> Vec<String> {
    columns
        .iter()
        .map(|column| match *column {
            "file" => normalize_file_path(row.get("file").map(String::as_str).unwrap_or("")),
            other => row.get(other).cloned().unwrap_or_default(),
        })
        .collect()
}

/// Assert every baseline open row appears in the actual CSV (recall).
pub fn assert_open_recall(
    baseline: &CsvTable,
    actual: &CsvTable,
    baseline_is_open: impl Fn(&HashMap<String, String>) -> bool,
    actual_is_open: impl Fn(&HashMap<String, String>) -> bool,
    key_columns: &[&str],
) {
    let actual_keys: BTreeSet<_> = actual
        .open_rows(actual_is_open)
        .into_iter()
        .map(|row| row_key(row, key_columns))
        .collect();

    let mut missing = Vec::new();
    for row in baseline.open_rows(baseline_is_open) {
        let key = row_key(row, key_columns);
        if !actual_keys.contains(&key) {
            missing.push(key);
        }
    }

    assert!(
        missing.is_empty(),
        "cordial output missing {} baseline open row(s):\n{missing:#?}\n\nactual keys:\n{actual_keys:#?}",
        missing.len()
    );
}

/// Assert cordial did not emit extra open rows beyond the baseline (precision).
pub fn assert_open_precision(
    baseline: &CsvTable,
    actual: &CsvTable,
    baseline_is_open: impl Fn(&HashMap<String, String>) -> bool,
    actual_is_open: impl Fn(&HashMap<String, String>) -> bool,
    key_columns: &[&str],
) {
    let baseline_keys: BTreeSet<_> = baseline
        .open_rows(baseline_is_open)
        .into_iter()
        .map(|row| row_key(row, key_columns))
        .collect();

    let mut extra = Vec::new();
    for row in actual.open_rows(actual_is_open) {
        let key = row_key(row, key_columns);
        if !baseline_keys.contains(&key) {
            extra.push(key);
        }
    }

    assert!(
        extra.is_empty(),
        "cordial emitted {} open row(s) not present in elicit_doc baseline:\n{extra:#?}",
        extra.len()
    );
}

pub fn parity_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity")
}

pub fn workspace_path(name: &str) -> PathBuf {
    parity_root().join("workspaces").join(name)
}

pub fn baseline_findings(name: &str, artifact: &str) -> PathBuf {
    parity_root()
        .join("baseline")
        .join(name)
        .join("findings")
        .join(artifact)
}

pub fn run_quality(workspace: &Path, store_root: &Path) -> miette::Result<()> {
    let mut builder = SessionBuilder::new(workspace).with_store_root(store_root);
    for etiquette in quality_etiquettes() {
        builder = builder.register(etiquette);
    }
    let session = builder.build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("cordial quality session")?;
    Ok(())
}

pub fn run_etiquette(
    workspace: &Path,
    store_root: &Path,
    etiquette: &'static dyn Etiquette,
) -> miette::Result<()> {
    let session = SessionBuilder::new(workspace)
        .with_store_root(store_root)
        .register(etiquette)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("cordial etiquette session")?;
    Ok(())
}

pub fn read_baseline_csv(workspace: &str, artifact: &str) -> miette::Result<CsvTable> {
    let path = baseline_findings(workspace, artifact);
    let content = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("read baseline {}", path.display()))?;
    CsvTable::parse(&content)
}

pub fn read_cordial_csv(store_root: &Path, artifact: &str) -> miette::Result<CsvTable> {
    let path = store_root.join("findings").join(artifact);
    let content = fs::read_to_string(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("read cordial output {}", path.display()))?;
    CsvTable::parse(&content)
}

pub fn panics_baseline_open(row: &HashMap<String, String>) -> bool {
    !row.get("kind").map(String::as_str).unwrap_or("").is_empty()
}

pub fn panics_cordial_open(row: &HashMap<String, String>) -> bool {
    panics_baseline_open(row)
}

pub fn tracing_baseline_open(row: &HashMap<String, String>) -> bool {
    if row.contains_key("disposition") {
        row.get("disposition").map(String::as_str) == Some("open")
    } else {
        row.get("instrumented").map(String::as_str) == Some("no")
    }
}

pub fn tracing_cordial_open(row: &HashMap<String, String>) -> bool {
    row.get("disposition").map(String::as_str) == Some("open")
}

pub const PANICS_KEY_COLUMNS: &[&str] = &["kind", "context", "file", "line"];
pub const TRACING_KEY_COLUMNS: &[&str] = &["qualified_name", "file", "line"];
pub const ALLOWS_KEY_COLUMNS: &[&str] = &["rule_id", "context", "file", "line"];
pub const DERIVES_KEY_COLUMNS: &[&str] = &["rule_id", "qualified_name", "file", "line"];
pub const ERROR_SITES_KEY_COLUMNS: &[&str] = &["site_kind", "context", "file", "line"];
pub const ERROR_CHAIN_KEY_COLUMNS: &[&str] = &["rule_id", "context", "file", "line"];
pub const INTERNAL_ERROR_COMPLIANCE_KEY_COLUMNS: &[&str] = &["rule_id", "context", "file", "line"];
pub const FOREIGN_ERROR_TYPES_KEY_COLUMNS: &[&str] = &[
    "foreign_error_type",
    "context",
    "file",
    "line",
    "chain_break",
];
pub const FOREIGN_ERROR_ATTENUATION_KEY_COLUMNS: &[&str] =
    &["handling_class", "context", "file", "line"];

pub fn allows_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("rule_id").is_some_and(|id| !id.is_empty())
}

pub fn allows_cordial_open(row: &HashMap<String, String>) -> bool {
    allows_baseline_open(row)
}

pub fn derives_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("rule_id").is_some_and(|id| !id.is_empty())
}

pub fn derives_cordial_open(row: &HashMap<String, String>) -> bool {
    derives_baseline_open(row)
}

pub fn error_sites_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("site_kind").is_some_and(|kind| !kind.is_empty())
}

pub fn error_sites_cordial_open(row: &HashMap<String, String>) -> bool {
    error_sites_baseline_open(row)
}

pub fn error_chain_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("rule_id").is_some_and(|id| !id.is_empty())
}

pub fn error_chain_cordial_open(row: &HashMap<String, String>) -> bool {
    error_chain_baseline_open(row)
}

pub fn internal_error_compliance_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("rule_id").is_some_and(|id| !id.is_empty())
}

pub fn internal_error_compliance_cordial_open(row: &HashMap<String, String>) -> bool {
    internal_error_compliance_baseline_open(row)
}

pub fn foreign_error_types_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("foreign_error_type")
        .is_some_and(|value| !value.is_empty())
}

pub fn foreign_error_types_cordial_open(row: &HashMap<String, String>) -> bool {
    foreign_error_types_baseline_open(row)
}

pub fn foreign_error_attenuation_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("handling_class")
        .is_some_and(|value| !value.is_empty())
}

pub fn foreign_error_attenuation_cordial_open(row: &HashMap<String, String>) -> bool {
    foreign_error_attenuation_baseline_open(row)
}

pub const ANTIPATTERNS_KEY_COLUMNS: &[&str] = &["rule_id", "context", "file", "line"];

pub fn antipatterns_baseline_open(row: &HashMap<String, String>) -> bool {
    row.get("rule_id").is_some_and(|id| !id.is_empty())
}

pub fn antipatterns_cordial_open(row: &HashMap<String, String>) -> bool {
    if row.contains_key("disposition") {
        row.get("disposition").map(String::as_str) == Some("open")
    } else {
        antipatterns_baseline_open(row)
    }
}

// ── Tier C coverage parity ───────────────────────────────────────────────────

pub const HOMECOMING_STD_KEY_COLUMNS: &[&str] = &["type_path", "trait_status"];
pub const AMENABLE_STD_KEY_COLUMNS: &[&str] = &["type_path", "status"];
pub const FRAMEWORK_GAPS_KEY_COLUMNS: &[&str] = &["type_path"];

/// True when `PARITY_TIER_C=1` (live workspace dual-run tests).
pub fn tier_c_enabled() -> bool {
    std::env::var("PARITY_TIER_C").ok().as_deref() == Some("1")
}

/// Resolve a Tier C workspace root from `env_var` or `default_path`.
pub fn tier_c_workspace(env_var: &str, default_path: &str) -> Option<PathBuf> {
    let path = std::env::var(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default_path));
    if path.is_dir() { Some(path) } else { None }
}

pub fn tier_c_baseline_findings(profile: &str, artifact: &str) -> PathBuf {
    parity_root()
        .join("baseline/tier_c")
        .join(profile)
        .join("findings")
        .join(artifact)
}

pub fn elicit_doc_manifest() -> PathBuf {
    std::env::var("ELICIT_DOC_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../elicit_doc/Cargo.toml")
        })
}

pub fn cordial_sysroot_ready(cordial_home: &Path) -> bool {
    ["std", "core", "alloc"].iter().all(|crate_name| {
        cordial_home
            .join("sysroot/cache/rustdoc")
            .join(format!("{crate_name}.json"))
            .is_file()
    })
}

/// Assert every row in `baseline` appears in `actual` with matching key columns (recall).
pub fn assert_csv_row_recall(
    baseline: &CsvTable,
    actual: &CsvTable,
    key_columns: &[&str],
    label: &str,
) {
    let actual_keys: BTreeSet<_> = actual
        .rows
        .iter()
        .map(|row| row_key(row, key_columns))
        .collect();

    let mut missing = Vec::new();
    for row in &baseline.rows {
        let key = row_key(row, key_columns);
        if !actual_keys.contains(&key) {
            missing.push(key);
        }
    }

    assert!(
        missing.is_empty(),
        "{label}: cordial missing {} baseline row(s):\n{missing:#?}",
        missing.len()
    );
}

/// Assert row sets match exactly on key columns (bidirectional).
pub fn assert_csv_row_sets_equal(
    baseline: &CsvTable,
    actual: &CsvTable,
    key_columns: &[&str],
    label: &str,
) {
    assert_csv_row_recall(baseline, actual, key_columns, label);
    assert_csv_row_recall(actual, baseline, key_columns, label);
}

#[cfg(feature = "homecoming_std")]
fn hub_patch_name(hub: cordial::WorkspaceHub) -> Option<&'static str> {
    match hub {
        cordial::WorkspaceHub::Homecoming => Some("homecoming"),
        cordial::WorkspaceHub::Amenable => Some("amenable"),
        _ => None,
    }
}

#[cfg(feature = "homecoming_std")]
fn elicit_doc_patch_seed(patch_name: &str) -> Option<String> {
    elicit_doc_manifest()
        .parent()
        .map(|root| {
            root.join("seeds")
                .join(patch_name)
                .join("patches")
                .join(format!("{patch_name}.json"))
        })
        .filter(|path| path.is_file())
        .and_then(|path| std::fs::read_to_string(path).ok())
}

#[cfg(feature = "homecoming_std")]
fn seed_patch_file(dest: &Path, content: &str) -> miette::Result<()> {
    if dest.is_file() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err("patch dir")?;
    }
    std::fs::write(dest, content)
        .into_diagnostic()
        .wrap_err("seed hub patch")?;
    Ok(())
}

/// Seed framework skip patches where elicit_doc's framework pipeline reads them.
///
/// `framework_stages` resolves `{output_dir.parent()}/patches/{patch_set}.json`,
/// not the project store's `patches/` tree, so Tier C dual-runs must mirror seeds
/// there as well as under cordial's store root.
#[cfg(feature = "homecoming_std")]
pub fn seed_elicit_framework_patches(
    coverage_dir: &Path,
    hub: cordial::WorkspaceHub,
) -> miette::Result<()> {
    let Some(patch_name) = hub_patch_name(hub) else {
        return Ok(());
    };
    let Some(content) = elicit_doc_patch_seed(patch_name) else {
        return Ok(());
    };
    let patches_dir = coverage_dir
        .parent()
        .unwrap_or(coverage_dir)
        .join("patches");
    seed_patch_file(&patches_dir.join(format!("{patch_name}.json")), &content)
}

#[cfg(feature = "homecoming_std")]
fn seed_hub_patches_if_needed(
    hub: cordial::WorkspaceHub,
    store: &cordial::StoreLayout,
) -> miette::Result<()> {
    let Some(patch_name) = hub_patch_name(hub) else {
        return Ok(());
    };
    let Some(content) = elicit_doc_patch_seed(patch_name) else {
        return Ok(());
    };
    seed_patch_file(
        &store
            .root
            .join("patches")
            .join(format!("{patch_name}.json")),
        &content,
    )
}

#[cfg(feature = "homecoming_std")]
fn ensure_sysroot_rustdoc_has_stability(sysroot: &cordial::SysrootCache) -> miette::Result<()> {
    use cordial::{BuildOptions, build_sysroot_libraries};

    let core_json = sysroot.rustdoc_cache_path("core");
    let needs_rebuild = !core_json.is_file()
        || std::fs::read_to_string(&core_json)
            .ok()
            .is_none_or(|content| !cordial::testing::rustdoc_json_has_stability_markers(&content));
    if needs_rebuild {
        build_sysroot_libraries(sysroot, None, &BuildOptions { force: true })
            .into_diagnostic()
            .wrap_err("build sysroot with stability-aware nightly")?;
    }
    Ok(())
}

#[cfg(feature = "homecoming_std")]
pub fn run_cordial_hub_coverage(
    workspace: &Path,
    store_root: &Path,
) -> miette::Result<cordial::WorkspaceHub> {
    use cordial::{
        BuildOptions, RunAll, SessionBuilder, StoreLayout, SysrootCache, build_workspace_members,
        coverage_plugins_for_hub, discover_workspace_hub, project_slug_from_path,
    };

    let sysroot = SysrootCache::default_cache();
    ensure_sysroot_rustdoc_has_stability(&sysroot)?;

    let hub = discover_workspace_hub(workspace, &RunAll)
        .into_diagnostic()
        .wrap_err("discover hub")?;
    let store = StoreLayout::from_root(store_root, project_slug_from_path(workspace));
    seed_hub_patches_if_needed(hub, &store)?;
    build_workspace_members(workspace, &store, None, &BuildOptions::default())
        .into_diagnostic()
        .wrap_err("build workspace rustdoc for coverage")?;

    let mut builder = SessionBuilder::new(workspace).with_store_root(store_root);
    for plugin in coverage_plugins_for_hub(hub) {
        builder = builder.register_plugin(plugin);
    }
    builder
        .build()
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("cordial coverage run")?;
    Ok(hub)
}

pub fn run_elicit_doc_coverage(
    project: &Path,
    store_home: &Path,
    coverage_dir: &Path,
) -> miette::Result<std::process::Output> {
    let manifest = elicit_doc_manifest();
    let manifest_dir = manifest
        .parent()
        .ok_or_else(|| miette::miette!("elicit_doc manifest directory"))?;
    assert!(
        manifest.is_file(),
        "elicit_doc manifest not found at {} — set ELICIT_DOC_MANIFEST",
        manifest.display()
    );

    std::process::Command::new("cargo")
        .current_dir(manifest_dir)
        .arg("run")
        .arg("-q")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("run")
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(store_home)
        .arg("--output-dir")
        .arg(coverage_dir)
        .output()
        .into_diagnostic()
        .wrap_err("spawn elicit_doc")
}

/// Run `elicit_doc quality {subcommand}` with isolated store + output dirs.
pub fn run_elicit_doc_quality(
    subcommand: &str,
    project: &Path,
    store_home: &Path,
    quality_dir: &Path,
) -> miette::Result<std::process::Output> {
    let manifest = elicit_doc_manifest();
    let manifest_dir = manifest
        .parent()
        .ok_or_else(|| miette::miette!("elicit_doc manifest directory"))?;
    assert!(
        manifest.is_file(),
        "elicit_doc manifest not found at {} — set ELICIT_DOC_MANIFEST",
        manifest.display()
    );

    std::process::Command::new("cargo")
        .current_dir(manifest_dir)
        .arg("run")
        .arg("-q")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("quality")
        .arg(subcommand)
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(store_home)
        .arg("--output-dir")
        .arg(quality_dir)
        .output()
        .into_diagnostic()
        .wrap_err("spawn elicit_doc quality")
}

#[cfg(feature = "antipatterns")]
pub fn run_cordial_antipatterns(workspace: &Path, store_root: &Path) -> miette::Result<()> {
    run_etiquette(workspace, store_root, &cordial::ANTIPATTERNS_ETIQUETTE)
}

//! CLI command bodies. Clap types in `cli` call these after `act` dispatch.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    CordialError, CordialResult, CrateIr, Disposition, Etiquette, Finding, NamedRunFilter, Plugin,
    ProgressSink, RunAll, RunFilter, RunOutcome, Session, SessionBuilder, StoreLayout,
    SurrealGraphExport, all_plugins, default_store_home, etiquettes_from_plugins, lookup_etiquette,
    render_explain_list, render_explain_page, run_tracing_instrument_apply,
};
use tracing::instrument;

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
#[cfg(any(feature = "elicitation", feature = "homecoming_std"))]
pub(super) fn execute_build_rustdoc(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    progress: Arc<dyn ProgressSink>,
    force: bool,
) -> CordialResult<()> {
    let artifacts = crate::cargo_rustdoc::build_workspace_members_with_progress(
        project_root,
        store,
        crate_name,
        force,
        progress.as_ref(),
    )?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %artifact.rustdoc_json().display(),
            "built rustdoc"
        );
    }
    #[cfg(feature = "elicitation")]
    if let Ok(shadow_dep_artifacts) =
        crate::cargo_rustdoc::build_all_active_shadow_deps_with_progress(
            project_root,
            store,
            force,
            progress.as_ref(),
        )
    {
        for artifact in shadow_dep_artifacts {
            tracing::info!(
                crate_name = artifact.crate_name(),
                via = artifact.reference_member().as_deref().unwrap_or("unknown"),
                path = %artifact.rustdoc_json().display(),
                "built shadow-dep rustdoc"
            );
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip(progress), err(level = "warn"))]
#[cfg(feature = "homecoming_std")]
pub(super) fn execute_build_sysroot(
    store_home: Option<PathBuf>,
    crate_name: Option<&str>,
    progress: Arc<dyn ProgressSink>,
    force: bool,
) -> CordialResult<()> {
    let home = store_home.unwrap_or_else(default_store_home);
    let sysroot = crate::SysrootCache::from_home(home);
    let artifacts = crate::cargo_rustdoc::build_sysroot_libraries_with_progress(
        &sysroot,
        crate_name,
        force,
        progress.as_ref(),
    )?;
    for artifact in artifacts {
        tracing::info!(
            crate_name = artifact.crate_name(),
            path = %sysroot.rustdoc_cache_path(artifact.crate_name()).display(),
            "built sysroot rustdoc"
        );
    }
    Ok(())
}

/// `cordial explain`: lists etiquettes for the selected project, so
/// etiquettes derived from its `cordial.toml` appear alongside built-ins.
#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn execute_explain(
    project_root: &Path,
    store_home: Option<PathBuf>,
    id: Option<&str>,
) -> CordialResult<()> {
    let session = SessionBuilder::new(project_root)
        .with_store_home(store_home.unwrap_or_else(default_store_home))
        .build();
    let plugins = all_plugins();
    let owned = etiquettes_from_plugins(&plugins, &session);
    let etiquettes: Vec<&dyn Etiquette> =
        owned.iter().map(|etiquette| etiquette.as_ref()).collect();
    match id {
        None => {
            write!(io::stdout(), "{}", render_explain_list(&etiquettes))?;
            Ok(())
        }
        Some(query) => {
            let Some(etiquette) = lookup_etiquette(&etiquettes, query) else {
                return Err(CordialError::unknown_etiquette(query));
            };
            write!(io::stdout(), "{}", render_explain_page(etiquette))?;
            Ok(())
        }
    }
}

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
pub(super) fn execute_quality_apply(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    store_home: Option<PathBuf>,
    checklist: Option<&Path>,
    progress: Arc<dyn ProgressSink>,
    dry_run: bool,
) -> CordialResult<()> {
    #[cfg(not(feature = "crate_attrs"))]
    let _ = store_home;

    #[cfg(feature = "crate_attrs")]
    {
        let home = store_home.clone().unwrap_or_else(default_store_home);
        let task = progress.spinner("Applying crate-level attributes".to_string());
        let summary = crate::run_crate_attrs_apply(project_root, &home, crate_name, dry_run)?;
        task.finish(format!(
            "Crate attributes applied to {} file(s)",
            summary.changed_files()
        ));
        tracing::info!(
            inserted_attrs = summary.inserted_attrs(),
            changed_files = summary.changed_files(),
            already_compliant = summary.skipped_existing(),
            unresolved = summary.unresolved(),
            "crate-attrs apply"
        );
    }

    #[cfg(feature = "tracing")]
    {
        let checklist_path = checklist
            .map(Path::to_path_buf)
            .unwrap_or_else(|| store.findings_dir().join("tracing-instrument.checklist.md"));
        if checklist_path.is_file() {
            let task = progress.spinner("Applying tracing instrumentation".to_string());
            execute_tracing_apply(
                project_root,
                store,
                crate_name,
                Some(&checklist_path),
                dry_run,
            )?;
            task.finish("Tracing instrumentation apply complete".to_string());
        } else {
            tracing::warn!(
                path = %checklist_path.display(),
                "tracing apply: no checklist, skipped"
            );
        }
    }

    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn execute_tracing_apply(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    checklist: Option<&Path>,
    dry_run: bool,
) -> CordialResult<()> {
    let checklist_path = checklist
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.findings_dir().join("tracing-instrument.checklist.md"));
    let summary = run_tracing_instrument_apply(project_root, &checklist_path, crate_name, dry_run)?;
    tracing::info!(
        changed_functions = summary.changed_functions(),
        changed_files = summary.changed_files(),
        already_instrumented = summary.skipped_existing(),
        skipped_by_policy = summary.skipped_policy(),
        unresolved = summary.unresolved(),
        "tracing apply"
    );
    Ok(())
}

#[instrument(level = "debug", skip(store, progress, plugins), err(level = "warn"))]
pub(super) fn execute_run_plugins(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    store_home: Option<PathBuf>,
    progress: Arc<dyn ProgressSink>,
    plugins: Vec<&'static dyn Plugin>,
    deny_open: bool,
) -> CordialResult<()> {
    let mut builder = SessionBuilder::new(project_root)
        .with_store_root(store.root().clone())
        .with_store_home(store_home.unwrap_or_else(default_store_home))
        .with_progress_sink(progress.clone());
    for plugin in plugins {
        builder = builder.register_plugin(plugin);
    }

    let session = builder.build();
    let filter = run_filter(crate_name);
    let outcome = session.run(filter.as_ref())?;
    let summary = print_run_summary(outcome.as_ref())?;
    progress.notice(format!(
        "Reports available under {}",
        store.findings_dir().display()
    ));
    if deny_open && summary.open_action_items > 0 {
        return Err(CordialError::open_findings(summary.open_action_items));
    }
    Ok(())
}

#[instrument(level = "info", fields(crate_name = crate_name))]
fn run_filter(crate_name: Option<&str>) -> RunFilterChoice {
    match crate_name {
        Some(crate_name) => {
            RunFilterChoice::Named(NamedRunFilter::all_plugins().with_crate(crate_name.to_string()))
        }
        None => RunFilterChoice::All(RunAll),
    }
}

enum RunFilterChoice {
    All(RunAll),
    Named(NamedRunFilter),
}

impl RunFilterChoice {
    #[instrument(level = "trace", skip(self))]
    fn as_ref(&self) -> &dyn RunFilter {
        match self {
            Self::All(filter) => filter,
            Self::Named(filter) => filter,
        }
    }
}

#[instrument(level = "debug", skip(outcome), err(level = "warn"))]
fn print_run_summary(outcome: &dyn RunOutcome) -> CordialResult<RunSummary> {
    let findings = outcome.findings().collect::<Vec<_>>();
    let mut open = 0usize;
    let mut exemplar = 0usize;
    let mut suppressed = 0usize;
    for finding in &findings {
        match finding.disposition() {
            Disposition::Open => open += 1,
            Disposition::Exemplar => exemplar += 1,
            Disposition::Suppressed => suppressed += 1,
        }
    }
    let open_action_items = open_action_items(&findings)?;
    let artifacts: Vec<_> = outcome
        .artifacts()
        .map(|artifact| artifact.name())
        .collect();
    tracing::info!(open, exemplar, suppressed, open_action_items, "findings");
    if !artifacts.is_empty() {
        tracing::info!(?artifacts, "artifacts");
    }
    Ok(RunSummary { open_action_items })
}

#[cfg(feature = "quality")]
#[instrument(level = "debug", skip(findings), err(level = "warn"))]
fn open_action_items(findings: &[&dyn Finding]) -> CordialResult<usize> {
    Ok(crate::build_quality_report(findings)?.total_open_items())
}

#[cfg(not(feature = "quality"))]
#[instrument(level = "debug", skip(findings))]
fn open_action_items(findings: &[&dyn Finding]) -> CordialResult<usize> {
    Ok(findings
        .iter()
        .filter(|finding| finding.disposition() == Disposition::Open)
        .count())
}

struct RunSummary {
    open_action_items: usize,
}

#[instrument(level = "debug", skip(store, path), err(level = "warn"))]
pub(super) fn view_store_file(store: &StoreLayout, path: &Path) -> CordialResult<()> {
    let full = store.root().join(path);
    if !full.is_file() {
        return Err(CordialError::not_found(full));
    }
    let bytes = fs::read(&full)?;
    io::stdout().write_all(&bytes)?;
    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
pub(super) fn export_surreal(
    store: &StoreLayout,
    project_root: &Path,
    crate_name: Option<&str>,
    output: Option<&Path>,
    statements: bool,
) -> CordialResult<()> {
    let crate_name = crate_name
        .map(str::to_string)
        .or_else(|| {
            crate::discover_crate_targets(project_root, &NamedRunFilter::all_plugins())
                .ok()
                .and_then(|targets| targets.into_iter().next().map(|t| t.crate_name().clone()))
        })
        .unwrap_or_else(|| store.project_slug().clone());
    let cache_path = store.ir_cache_path(&crate_name);
    if !cache_path.is_file() {
        return Err(CordialError::no_cached_ir(cache_path));
    }
    let ir = CrateIr::read_cache(&cache_path)?;
    let export = SurrealGraphExport::from_crate_ir(&ir)?;
    let body = if statements {
        crate::surreal_statements(&export).join("\n") + "\n"
    } else {
        export.to_json_pretty()?
    };

    if let Some(path) = output {
        fs::write(path, body)?;
        tracing::info!(path = %path.display(), "wrote surreal export");
    } else {
        write!(io::stdout(), "{body}")?;
    }
    Ok(())
}

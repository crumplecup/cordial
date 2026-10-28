//! Source transformations and the worklist claims produced by successful edits.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{CordialResult, ProgressSink, StoreLayout, default_store_home};
use tracing::instrument;

#[instrument(level = "debug", skip(store, progress), err(level = "warn"))]
pub(in crate::cli) fn execute_quality_apply(
    project_root: &Path,
    store: &StoreLayout,
    crate_name: Option<&str>,
    store_home: Option<PathBuf>,
    checklist: Option<&Path>,
    progress: Arc<dyn ProgressSink>,
    dry_run: bool,
    only_etiquette: Option<&str>,
) -> CordialResult<()> {
    let _worklist_lock =
        crate::WorklistLock::exclusive(store, "quality --apply", progress.as_ref())?;
    #[cfg(not(any(feature = "crate_attrs", feature = "internal_error_chain")))]
    let _ = store_home;

    #[cfg(feature = "crate_attrs")]
    if only_etiquette.is_none_or(|name| name == "crate_attrs") {
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
    if only_etiquette.is_none_or(|name| name == "tracing") {
        let home = store_home.clone().unwrap_or_else(default_store_home);
        if let Some(path) = checklist.filter(|path| !path.is_file()) {
            tracing::warn!(
                path = %path.display(),
                "tracing apply: no checklist, skipped"
            );
        } else {
            let task = progress.spinner("Applying tracing instrumentation".to_string());
            execute_tracing_apply(project_root, store, &home, crate_name, checklist, dry_run)?;
            task.finish("Tracing instrumentation apply complete".to_string());
        }
    }

    #[cfg(feature = "internal_error_chain")]
    if only_etiquette.is_none_or(|name| name == "internal_error_chain") {
        let home = store_home.clone().unwrap_or_else(default_store_home);
        let task = progress.spinner("Applying error field storage policy".to_string());
        let summary = crate::run_error_field_apply(project_root, &home, crate_name, dry_run)?;
        task.finish(format!(
            "Error field storage converted for {} field(s)",
            summary.converted_fields()
        ));
        tracing::info!(
            changed_files = summary.changed_files(),
            converted_fields = summary.converted_fields(),
            unresolved = summary.unresolved(),
            "error file apply"
        );
    }

    Ok(())
}

#[instrument(level = "debug", skip(store), err(level = "warn"))]
fn execute_tracing_apply(
    project_root: &Path,
    store: &StoreLayout,
    store_home: &Path,
    crate_name: Option<&str>,
    checklist: Option<&Path>,
    dry_run: bool,
) -> CordialResult<()> {
    use crate::etiquettes::tracing::{ApplyTargets, run_apply};

    let targets = if let Some(path) = checklist {
        ApplyTargets::Checklist(path)
    } else {
        ApplyTargets::CurrentIr(store)
    };
    let summary = run_apply(project_root, store_home, targets, crate_name, dry_run)?;
    tracing::info!(
        changed_functions = summary.changed_functions(),
        changed_files = summary.changed_files(),
        already_instrumented = summary.skipped_existing(),
        skipped_by_policy = summary.skipped_policy(),
        unresolved = summary.unresolved(),
        "tracing apply"
    );
    #[cfg(feature = "quality")]
    report_tracing_apply_worklist(project_root, store, store_home, &summary, dry_run)?;
    Ok(())
}

#[cfg(feature = "quality")]
#[instrument(level = "debug", skip(store, summary), err(level = "warn"))]
fn report_tracing_apply_worklist(
    project_root: &Path,
    store: &StoreLayout,
    store_home: &Path,
    summary: &crate::etiquettes::tracing::InstrumentApplySummary,
    dry_run: bool,
) -> CordialResult<()> {
    use std::io::{self, Write};

    let Some(mut list) = crate::Worklist::read(store)? else {
        writeln!(
            io::stdout(),
            "Tracing apply: no current worklist; run `cordial open` to track remaining items."
        )?;
        return Ok(());
    };
    let config = crate::load_cordial_config(project_root, store_home);
    if list.validity(&config)? != crate::Validity::Current {
        writeln!(
            io::stdout(),
            "Tracing apply: worklist is out of date; run `cordial open` to track remaining items."
        )?;
        return Ok(());
    }

    let ids: Vec<String> = list
        .items()
        .iter()
        .filter(|item| *item.state() != crate::ItemState::Done)
        .filter(|item| item.checklist() == "tracing-instrument.checklist.md")
        .filter(|item| {
            summary.applied_gaps().iter().any(|gap| {
                item.crate_name().as_deref() == Some(gap.crate_name())
                    && item.file().as_deref() == gap.rel_path().to_str()
                    && item.line().as_ref() == Some(&gap.line())
                    && item.item_line().split('`').nth(1) == Some(gap.qualified_name())
            })
        })
        .map(|item| item.id().as_str().to_string())
        .collect();
    if !dry_run && !ids.is_empty() {
        list.done(&ids)?;
        list.write(store)?;
    }
    let remaining = list
        .items()
        .iter()
        .filter(|item| item.checklist().starts_with("tracing-"))
        .filter(|item| *item.state() == crate::ItemState::Open)
        .count();
    writeln!(
        io::stdout(),
        "Tracing apply: {} worklist item(s) checked off; {remaining} tracing item(s) remain open for manual repair.{}",
        if dry_run { 0 } else { ids.len() },
        if dry_run {
            " Dry run: no worklist changes."
        } else {
            ""
        }
    )?;
    Ok(())
}

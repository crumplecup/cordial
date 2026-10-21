//! Optional per-file source work measurements for combined pipeline runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{CordialError, CordialResult};

#[derive(Serialize)]
struct SourceWorkEntry {
    file: PathBuf,
    hook: String,
    operation: String,
    count: u64,
    elapsed_nanos: u128,
}

type Key = (PathBuf, String, String);
static OUTPUT: OnceLock<Option<PathBuf>> = OnceLock::new();
static ENTRIES: LazyLock<Mutex<BTreeMap<Key, (u64, u128)>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[tracing::instrument(level = "debug")]
fn output() -> Option<&'static PathBuf> {
    OUTPUT
        .get_or_init(|| std::env::var_os("CORDIAL_SOURCE_WORK_PATH").map(PathBuf::from))
        .as_ref()
}

#[tracing::instrument(level = "debug", skip(path))]
fn record(path: &Path, hook: &str, operation: &str, elapsed: Duration) {
    if output().is_none() {
        return;
    }
    let mut entries = ENTRIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = entries
        .entry((path.to_path_buf(), hook.to_string(), operation.to_string()))
        .or_default();
    entry.0 += 1;
    entry.1 += elapsed.as_nanos();
}

#[tracing::instrument(level = "debug")]
pub(crate) fn reset() {
    if output().is_some() {
        ENTRIES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(path))]
pub(crate) fn read(path: &Path, hook: &str) -> CordialResult<String> {
    let started = Instant::now();
    let result = std::fs::read_to_string(path);
    record(path, hook, "read", started.elapsed());
    result.map_err(Into::into)
}

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(source, path))]
pub(crate) fn parse(source: &str, path: &Path, hook: &str) -> CordialResult<syn::File> {
    let started = Instant::now();
    let result = syn::parse_file(source);
    record(path, hook, "parse", started.elapsed());
    result.map_err(|error| CordialError::syn_parse(path.display().to_string(), error))
}

#[tracing::instrument(level = "debug", skip(path, action))]
pub(crate) fn walk<T>(path: &Path, hook: &str, action: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = action();
    record(path, hook, "walk", started.elapsed());
    result
}

#[tracing::instrument(level = "debug", skip(path))]
pub(crate) fn reuse(path: &Path, hook: &str) {
    record(path, hook, "reuse", Duration::ZERO);
}

#[tracing::instrument(level = "debug")]
pub(crate) fn write() -> CordialResult<()> {
    let Some(path) = output() else {
        return Ok(());
    };
    let entries = ENTRIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rows: Vec<_> = entries
        .iter()
        .map(
            |((file, hook, operation), (count, elapsed_nanos))| SourceWorkEntry {
                file: file.clone(),
                hook: hook.clone(),
                operation: operation.clone(),
                count: *count,
                elapsed_nanos: *elapsed_nanos,
            },
        )
        .collect();
    std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}

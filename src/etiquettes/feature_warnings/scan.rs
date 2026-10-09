//! Run the feature powerset and collect warnings that depend on features.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::config::FeatureWarningsThresholds;
use crate::error::CordialResult;

use super::gate::{FeatureSet, Gate, suggest_gate};
use super::types::{FeatureWarningRecord, FeatureWarningRuleId};

use tracing::instrument;

/// Where a warning fires: `(file, line, lint code)`.
type SiteKey = (String, u32, String);

/// What the powerset run saw at one site.
#[derive(Debug, Default, Clone)]
struct SiteHits {
    combos: BTreeSet<FeatureSet>,
    names: BTreeSet<String>,
    message: String,
}

/// What the powerset run saw at one compile-error site.
#[derive(Debug, Default, Clone)]
struct FailureHits {
    /// Descriptions of the failed combinations (`--no-default-features ...`).
    combos: BTreeSet<String>,
    message: String,
}

/// Parsed output of one `cargo hack check --message-format=json` run.
#[derive(Debug, Default, Clone)]
pub struct HackRun {
    combos: BTreeSet<FeatureSet>,
    sites: BTreeMap<SiteKey, SiteHits>,
    /// One description per invocation whose build failed.
    failed_combos: Vec<String>,
    failures: BTreeMap<SiteKey, FailureHits>,
    /// `cargo hack` itself failed without any build error to point at.
    hack_failure: Option<String>,
}

impl HackRun {
    /// Number of distinct feature combinations that compiled.
    #[instrument(level = "trace", skip(self))]
    pub fn combination_count(&self) -> usize {
        self.combos.len()
    }

    /// Number of invocations whose build failed.
    #[instrument(level = "trace", skip(self))]
    pub fn failed_combination_count(&self) -> usize {
        self.failed_combos.len()
    }

    /// Record that `cargo hack` exited unsuccessfully. Only meaningful when
    /// no failed build explains it (a dependency or manifest problem); the
    /// `error` lines of `stderr` become the finding's message.
    #[instrument(level = "debug", skip(self, stderr))]
    pub fn with_hack_failure(mut self, stderr: &str) -> Self {
        if self.failures.is_empty() {
            let lines: Vec<&str> = stderr
                .lines()
                .filter(|line| line.starts_with("error"))
                .collect();
            let message = if lines.is_empty() {
                "cargo hack exited unsuccessfully".to_string()
            } else {
                lines.join(" | ")
            };
            self.hack_failure = Some(message);
        }
        self
    }
}

/// Raw output of one `cargo hack` invocation.
#[derive(Debug)]
struct HackOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

/// Scan a crate: run the powerset unless this package is skipped or
/// `cargo hack` is not installed.
///
/// `resolve_root` is the base a reported diagnostic path is joined against
/// (cargo reports paths relative to the workspace root), as in
/// `scan_crate_doc_warnings`.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_feature_warnings(
    crate_root: &Path,
    resolve_root: &Path,
    crate_name: &str,
    policy: &FeatureWarningsThresholds,
) -> CordialResult<Vec<FeatureWarningRecord>> {
    if policy.skip(crate_name) || !crate_root.join("Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    if !cargo_hack_available(crate_root) {
        tracing::debug!("cargo-hack not available; skipping feature warning scan");
        return Ok(Vec::new());
    }
    let output = run_cargo_hack(crate_root, crate_name, policy)?;
    let mut run = parse_cargo_hack_run(&output.stdout, &output.stderr, crate_name);
    if !output.success {
        run = run.with_hack_failure(&output.stderr);
    }
    let mut ignore: BTreeSet<String> = policy.exclude_features().iter().cloned().collect();
    ignore.insert("default".to_string());
    records_from_run(
        &run,
        resolve_root,
        policy.include_universal(),
        &ignore,
        policy.private_feature_threshold(),
    )
}

/// Parse `cargo hack check --message-format=json` stdout for `package`,
/// without stderr: a failed build cannot be named, so it is reported as an
/// unknown combination.
#[instrument(level = "debug", skip(output))]
pub fn parse_cargo_hack_output(output: &str, package: &str) -> HackRun {
    parse_cargo_hack_run(output, "", package)
}

/// Parse a `cargo hack check --message-format=json` run for `package`.
///
/// Compiler messages carry no feature set, but each package's
/// `compiler-artifact` record does. Warnings are buffered until that
/// package's next artifact and tagged with its features.
///
/// A build that fails produces no artifact, so it is found by its
/// `build-finished` record with `success: false`. Its feature set is not in
/// the JSON; `cargo hack` prints `info: running` lines on stderr once per
/// invocation, in the same order as the `build-finished` records, so the Nth
/// finished build takes the Nth description.
#[instrument(level = "debug", skip(stdout, stderr))]
pub fn parse_cargo_hack_run(stdout: &str, stderr: &str, package: &str) -> HackRun {
    let invocations = invocation_descriptions(stderr);
    let mut run = HackRun::default();
    let mut warnings: Vec<(SiteKey, String)> = Vec::new();
    let mut errors: Vec<(SiteKey, String)> = Vec::new();
    let mut finished = 0usize;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let reason = value.get("reason").and_then(serde_json::Value::as_str);
        if reason == Some("build-finished") {
            if value.get("success").and_then(serde_json::Value::as_bool) == Some(false) {
                let label = invocations
                    .get(finished)
                    .cloned()
                    .unwrap_or_else(|| "(unknown combination)".to_string());
                run.failed_combos.push(label.clone());
                for (key, message) in errors.drain(..) {
                    let hits = run.failures.entry(key).or_default();
                    hits.combos.insert(label.clone());
                    if hits.message.is_empty() {
                        hits.message = message;
                    }
                }
            }
            // Whatever this build buffered is settled; never let it leak
            // into the next combination's artifact.
            warnings.clear();
            errors.clear();
            finished += 1;
            continue;
        }
        let Some(id) = value.get("package_id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if package_name(id) != package {
            continue;
        }
        match reason {
            Some("compiler-message") => {
                if let Some(site) = diagnostic_site(&value, "warning") {
                    warnings.push(site);
                } else if let Some(site) = diagnostic_site(&value, "error") {
                    errors.push(site);
                }
            }
            Some("compiler-artifact") => {
                let combo = artifact_features(&value);
                run.combos.insert(combo.clone());
                for (key, message) in warnings.drain(..) {
                    let hits = run.sites.entry(key).or_default();
                    hits.combos.insert(combo.clone());
                    hits.names.extend(backticked(&message));
                    if hits.message.is_empty() {
                        hits.message = message;
                    }
                }
            }
            _ => {}
        }
    }
    run
}

/// The `cargo check ...` arguments of each `info: running` line, in order.
#[instrument(level = "trace", skip(stderr))]
fn invocation_descriptions(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("info: running `cargo ")?;
            let command = rest.split('`').next()?;
            // cargo-hack appends pass-through arguments (`--message-format`,
            // `--target-dir <path>`) before the feature flags; keep only the
            // part that says which features were on.
            let start = ["--no-default-features", "--all-features", "--features"]
                .iter()
                .filter_map(|flag| command.find(flag))
                .min()
                .unwrap_or(0);
            Some(command[start..].trim().to_string())
        })
        .collect()
}

/// Turn a parsed run into one record per feature-dependent site.
#[instrument(level = "debug", skip(run), err(level = "warn"))]
pub fn records_from_run(
    run: &HackRun,
    resolve_root: &Path,
    include_universal: bool,
    ignore_features: &BTreeSet<String>,
    private_feature_threshold: usize,
) -> CordialResult<Vec<FeatureWarningRecord>> {
    let total = run.combos.len();
    let mut records = Vec::new();
    for ((file, line, lint), hits) in &run.sites {
        let silent: Vec<FeatureSet> = run.combos.difference(&hits.combos).cloned().collect();
        if silent.is_empty() && !include_universal {
            continue;
        }
        let triggering: Vec<FeatureSet> = hits.combos.iter().cloned().collect();
        let gate = suggest_gate(
            &without_features(&triggering, ignore_features),
            &compiled_silent(&triggering, &silent, ignore_features),
        );
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::for_lint(lint))
                .lint(lint.clone())
                .file(resolve_root.join(file))
                .line(*line)
                .message(site_message(lint, hits))
                .gate(gate.cfg_expression().unwrap_or_default())
                .wide(gate.feature_count() > private_feature_threshold)
                .advice(advice_for(lint, &gate, private_feature_threshold))
                .triggering(describe_triggering(&triggering, total))
                .build()?,
        );
    }
    let attempted = total + run.failed_combos.len();
    for ((file, line, code), hits) in &run.failures {
        let example = hits
            .combos
            .iter()
            .min_by_key(|combo| combo.len())
            .map(|combo| format!(", e.g. `{combo}`"))
            .unwrap_or_default();
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::Failure003)
                .lint(code.clone())
                .file(resolve_root.join(file))
                .line(*line)
                .message(hits.message.clone())
                .gate(String::new())
                .wide(false)
                .advice(failure_advice(code))
                .triggering(format!(
                    "fails in {} of {attempted} combinations{example}",
                    hits.combos.len()
                ))
                .build()?,
        );
    }
    if let Some(message) = &run.hack_failure {
        records.push(
            FeatureWarningRecord::builder()
                .rule_id(FeatureWarningRuleId::Failure003)
                .lint("cargo-hack".to_string())
                .file(resolve_root.join("Cargo.toml"))
                .line(1)
                .message(message.clone())
                .gate(String::new())
                .wide(false)
                .advice(
                    "`cargo hack` failed before or outside any build, so no feature \
                     combination could be checked. Run `cargo hack check \
                     --feature-powerset` by hand to see the full error; a manifest or \
                     dependency problem is the usual cause."
                        .to_string(),
                )
                .triggering("the whole run".to_string())
                .build()?,
        );
    }
    records.sort_by(|left, right| {
        left.file()
            .cmp(right.file())
            .then(left.line().cmp(&right.line()))
            .then(left.lint().cmp(right.lint()))
    });
    Ok(records)
}

/// What to do about a combination that does not compile.
#[instrument(level = "debug")]
pub fn failure_advice(code: &str) -> String {
    let cause = match code {
        "E0432" | "E0433" | "E0412" | "E0425" | "E0405" | "E0599" | "E0603" => {
            "A name used here is gated out in these combinations: the definition sits \
             behind a feature that these combinations do not enable."
        }
        _ => "The compiler rejects this code in these combinations.",
    };
    format!(
        "{cause} Fix it before anything else: warnings in a combination that does \
         not compile cannot be assessed. Either make the feature that gates the \
         definition imply the one that gates this use (in `Cargo.toml`), or give this \
         use the same `cfg` as the definition."
    )
}

/// `combos` with `ignored` features removed. Umbrella features (`default`,
/// `full`, ...) are not something a `cfg` gate should name.
#[instrument(level = "trace", skip(combos))]
fn without_features(combos: &[FeatureSet], ignored: &BTreeSet<String>) -> Vec<FeatureSet> {
    combos
        .iter()
        .map(|combo| combo.difference(ignored).cloned().collect())
        .collect()
}

/// The silent combinations in which the item is compiled at all.
///
/// Silence has two causes: the item is used, or the item's own gate is off so
/// it does not exist. Only the first says anything about the consumer. The
/// item exists wherever it can warn, so a silent combination counts when it
/// contains every feature common to the triggering ones.
#[instrument(level = "trace", skip(triggering, silent))]
fn compiled_silent(
    triggering: &[FeatureSet],
    silent: &[FeatureSet],
    ignored: &BTreeSet<String>,
) -> Vec<FeatureSet> {
    let triggering = without_features(triggering, ignored);
    let silent = without_features(silent, ignored);
    let Some(first) = triggering.first() else {
        return silent;
    };
    let common = triggering.iter().skip(1).fold(first.clone(), |acc, combo| {
        acc.intersection(combo).cloned().collect()
    });
    silent
        .into_iter()
        .filter(|combo| common.is_subset(combo))
        .collect()
}

#[instrument(level = "trace")]
fn package_name(package_id: &str) -> String {
    // Old form: `cordial 0.1.0 (path+file:///...)`.
    if let Some((name, _)) = package_id.split_once(' ') {
        return name.to_string();
    }
    // New form: `path+file:///a/b/cordial#0.1.0` or `...#name@1.2.3`.
    let (location, fragment) = package_id.split_once('#').unwrap_or((package_id, ""));
    if let Some((name, _)) = fragment.split_once('@') {
        return name.to_string();
    }
    location.rsplit('/').next().unwrap_or(location).to_string()
}

#[instrument(level = "trace", skip(value))]
fn artifact_features(value: &serde_json::Value) -> FeatureSet {
    value
        .get("features")
        .and_then(serde_json::Value::as_array)
        .map(|features| {
            features
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The `(site, message)` of a diagnostic at `level` with a source span;
/// `None` for other levels and for the span-less "N warnings emitted" and
/// "could not compile" summaries. Warnings must carry a lint code; errors
/// without one (a syntax error) are keyed as `error`.
#[instrument(level = "trace", skip(value))]
fn diagnostic_site(value: &serde_json::Value, level: &str) -> Option<(SiteKey, String)> {
    let message = value.get("message")?;
    if message.get("level")?.as_str()? != level {
        return None;
    }
    let code = message
        .get("code")
        .and_then(|code| code.get("code"))
        .and_then(serde_json::Value::as_str);
    let lint = match (level, code) {
        (_, Some(code)) => code.to_string(),
        ("error", None) => "error".to_string(),
        _ => return None,
    };
    let text = message.get("message")?.as_str()?.trim().to_string();
    let spans = message.get("spans")?.as_array()?;
    let primary = spans
        .iter()
        .find(|span| span.get("is_primary").and_then(serde_json::Value::as_bool) == Some(true))
        .or_else(|| spans.first())?;
    let file = primary.get("file_name")?.as_str()?.to_string();
    let line = u32::try_from(primary.get("line_start")?.as_u64()?).ok()?;
    Some(((file, line, lint), text))
}

#[instrument(level = "trace")]
fn backticked(message: &str) -> Vec<String> {
    message
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The message shown for a site. Unused-import messages list different names
/// per combination, so merge them; every other lint keeps its own text.
#[instrument(level = "trace", skip(hits))]
fn site_message(lint: &str, hits: &SiteHits) -> String {
    if lint != "unused_imports" || hits.names.len() < 2 {
        return hits.message.clone();
    }
    let names: Vec<String> = hits.names.iter().map(|name| format!("`{name}`")).collect();
    format!("unused imports: {}", names.join(", "))
}

#[instrument(level = "trace")]
fn describe_combo(combo: &FeatureSet) -> String {
    if combo.is_empty() {
        return "--no-default-features".to_string();
    }
    let features: Vec<&str> = combo.iter().map(String::as_str).collect();
    format!("--no-default-features --features {}", features.join(","))
}

#[instrument(level = "trace", skip(triggering))]
fn describe_triggering(triggering: &[FeatureSet], total: usize) -> String {
    let example = triggering
        .iter()
        .min_by_key(|combo| (combo.len(), (*combo).clone()));
    match example {
        Some(combo) => format!(
            "{} of {total} combinations, e.g. `{}`",
            triggering.len(),
            describe_combo(combo)
        ),
        None => format!("0 of {total} combinations"),
    }
}

/// What to do about a warning, given the lint and the suggested gate.
#[instrument(level = "debug")]
pub fn advice_for(lint: &str, gate: &Gate, private_feature_threshold: usize) -> String {
    if gate.feature_count() > private_feature_threshold {
        return format!(
            "{} features reach this item, too many for a readable `cfg`. Add a \
             private feature (name it with a leading `_`, e.g. `_<area>_support`) \
             that each of them enables, and gate on that; or move the item next \
             to its only consumer if it has one.",
            gate.feature_count()
        );
    }
    let Some(cfg) = gate.cfg_expression() else {
        return "No single feature gate separates the combinations. If several \
                features share this item, give them a common internal feature; \
                if one consumer uses it, move the item next to that consumer."
            .to_string();
    };
    let what = match lint {
        "unused_imports" => {
            "Gate the `use` (or move it into the gated module that uses it)".to_string()
        }
        "dead_code" => {
            "Gate the definition, or move it into the file of its only consumer".to_string()
        }
        "unused_variables" | "unused_mut" => {
            "Gate the statement, or the `mut`, that only those features need".to_string()
        }
        _ => "Gate the item".to_string(),
    };
    format!("{what} with `#[cfg({cfg})]`.")
}

/// The package's private features: those named with a leading `_`. They are
/// only ever enabled by other features, so the powerset skips them as units.
#[instrument(level = "debug")]
fn private_features(crate_root: &Path, crate_name: &str) -> Vec<String> {
    let Ok(metadata) = cargo_metadata::MetadataCommand::new()
        .current_dir(crate_root)
        .no_deps()
        .exec()
    else {
        return Vec::new();
    };
    metadata
        .packages
        .iter()
        .filter(|package| package.name.as_str() == crate_name)
        .flat_map(|package| package.features.keys())
        .filter(|feature| feature.starts_with('_'))
        .cloned()
        .collect()
}

#[instrument(level = "debug")]
fn cargo_command() -> Command {
    let cargo: OsString = std::env::var_os("CORDIAL_CARGO")
        .or_else(|| std::env::var_os("CARGO"))
        .unwrap_or_else(|| OsString::from("cargo"));
    Command::new(cargo)
}

#[instrument(level = "debug")]
fn cargo_hack_available(crate_root: &Path) -> bool {
    cargo_command()
        .current_dir(crate_root)
        .args(["hack", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[instrument(level = "info", skip(policy), fields(crate_name = crate_name))]
fn run_cargo_hack(
    crate_root: &Path,
    crate_name: &str,
    policy: &FeatureWarningsThresholds,
) -> CordialResult<HackOutput> {
    let target_dir = crate_root.join("target").join("feature-warnings");
    let mut command = cargo_command();
    command
        .current_dir(crate_root)
        // Every combination is a cold cache; incremental only costs memory.
        .env("CARGO_INCREMENTAL", "0")
        .args(["hack", "check", "--feature-powerset", "--keep-going"])
        .arg("--depth")
        .arg(policy.depth().to_string())
        .arg("--message-format=json")
        .arg("-p")
        .arg(crate_name)
        .arg("--target-dir")
        .arg(&target_dir);
    let mut excluded: Vec<String> = policy.exclude_features().to_vec();
    excluded.extend(private_features(crate_root, crate_name));
    if !excluded.is_empty() {
        command.arg("--exclude-features").arg(excluded.join(","));
    }
    for group in policy.group_features() {
        command.arg("--group-features").arg(group.join(","));
    }
    let output = command.output()?;
    Ok(HackOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    })
}

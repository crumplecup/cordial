//! Parse `cargo hack check --message-format=json` output into a [`HackRun`].

use super::{HackRun, SiteKey};
use crate::etiquettes::feature_warnings::gate::FeatureSet;

use tracing::instrument;

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

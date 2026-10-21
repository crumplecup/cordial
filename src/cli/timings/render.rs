//! Human-readable timing report.

use std::collections::BTreeSet;
use tracing::instrument;

use super::marginal::MarginalPair;
use super::{TimingEntry, TimingPass, TimingReport};

#[instrument(level = "trace", skip(report))]
pub(super) fn render_markdown(report: &TimingReport) -> String {
    let mut body = format!(
        "# Etiquette timings\n\nProject: `{}`  \nMeasured: Unix {}\n\nEach etiquette ran alone. Cold and warm passes used the same isolated Cargo target. Durations include session setup, hooks, and rendering.\n\n| Etiquette | Cold (s) | Warm (s) |\n| --- | ---: | ---: |\n",
        report.project, report.measured_at_unix_secs
    );
    for entry in &report.entries {
        let cold = entry
            .cold
            .as_ref()
            .map(|pass| pass.wall_time_nanos as f64 / 1_000_000_000.0);
        let warm = entry
            .warm
            .as_ref()
            .map(|pass| pass.wall_time_nanos as f64 / 1_000_000_000.0);
        body.push_str(&format!(
            "| `{}` | {} | {} |\n",
            entry.id,
            cold.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
            warm.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
        ));
    }
    body.push_str("\n## Pipeline phases\n\nDurations are inclusive; phase rows overlap with hook and pipeline detail in `timings.json`.\n\n| Etiquette | Phase | Cold (s) | Warm (s) |\n| --- | --- | ---: | ---: |\n");
    for entry in &report.entries {
        let phases: BTreeSet<&str> = entry
            .cold
            .iter()
            .chain(entry.warm.iter())
            .flat_map(|pass| pass.hooks.iter())
            .filter(|hook| hook.stage() == "phase")
            .map(|hook| hook.id().as_str())
            .collect();
        for phase in phases {
            let seconds = |pass: &TimingPass| {
                pass.hooks
                    .iter()
                    .find(|hook| hook.stage() == "phase" && hook.id() == phase)
                    .map(|hook| *hook.elapsed_nanos() as f64 / 1_000_000_000.0)
            };
            let cold = entry.cold.as_ref().and_then(seconds);
            let warm = entry.warm.as_ref().and_then(seconds);
            body.push_str(&format!(
                "| `{}` | `{phase}` | {} | {} |\n",
                entry.id,
                cold.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
                warm.map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
            ));
        }
    }
    if report.version >= 2 {
        body.push_str("\n## Marginal cost (full minus without)\n\nEach pair uses separate empty Cargo targets for full and without, then repeats both with their targets warm. Overlap is standalone time minus marginal time: it mixes shared startup, shared hooks, and interactions, so it is not an exact allocation of shared cost. Negative values can occur. Stage rows are inclusive and must not be summed. Rules listed below share their etiquette's measured cost; these are not independently measured rule costs.\n\n| Etiquette | Rules | Cold delta (s) | Warm delta (s) | Warm overlap (s) |\n| --- | --- | ---: | ---: | ---: |\n");
        for entry in &report.entries {
            let Some(measurement) = &entry.marginal else {
                continue;
            };
            let seconds = |pair: &MarginalPair| *pair.wall_delta_nanos() as f64 / 1_000_000_000.0;
            body.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                entry.id,
                entry
                    .rules
                    .iter()
                    .map(|rule| format!("`{rule}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                measurement
                    .cold
                    .as_ref()
                    .map_or_else(|| "n/a".to_string(), |pair| format!("{:.2}", seconds(pair))),
                measurement
                    .warm
                    .as_ref()
                    .map_or_else(|| "n/a".to_string(), |pair| format!("{:.2}", seconds(pair))),
                measurement.warm_overlap_nanos.map_or_else(
                    || "n/a".to_string(),
                    |nanos| format!("{:.2}", nanos as f64 / 1_000_000_000.0)
                ),
            ));
        }
        body.push_str("\n### Marginal pipeline phases\n\n| Etiquette | Phase | Cold delta (s) | Warm delta (s) |\n| --- | --- | ---: | ---: |\n");
        for entry in &report.entries {
            let Some(measurement) = &entry.marginal else {
                continue;
            };
            let phases: BTreeSet<&str> = measurement
                .cold
                .iter()
                .chain(measurement.warm.iter())
                .flat_map(|pair| pair.stage_deltas().iter())
                .filter(|delta| delta.stage() == "phase")
                .map(|delta| delta.id().as_str())
                .collect();
            for phase in phases {
                let seconds = |pair: &MarginalPair| {
                    pair.stage_deltas()
                        .iter()
                        .find(|delta| delta.stage() == "phase" && delta.id() == phase)
                        .map(|delta| *delta.elapsed_nanos() as f64 / 1_000_000_000.0)
                };
                body.push_str(&format!(
                    "| `{}` | `{phase}` | {} | {} |\n",
                    entry.id,
                    measurement
                        .cold
                        .as_ref()
                        .and_then(seconds)
                        .map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
                    measurement
                        .warm
                        .as_ref()
                        .and_then(seconds)
                        .map_or_else(|| "n/a".to_string(), |value| format!("{value:.2}")),
                ));
            }
        }
    }
    let unavailable: Vec<&TimingEntry> = report
        .entries
        .iter()
        .filter(|entry| entry.error.is_some())
        .collect();
    if !unavailable.is_empty() {
        body.push_str("\n## Unavailable measurements\n\n");
        for entry in unavailable {
            body.push_str(&format!(
                "- `{}`: {}\n",
                entry.id,
                entry.error.as_deref().unwrap_or_default()
            ));
        }
    }
    for entry in &report.entries {
        if let Some(error) = entry
            .marginal
            .as_ref()
            .and_then(|measurement| measurement.error.as_ref())
        {
            body.push_str(&format!("\n- Marginal `{}`: {error}\n", entry.id));
        }
    }
    body
}

//! Paired full versus omitted etiquette measurements.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use tracing::instrument;

use super::pass::run_child;
use super::{MarginalMeasurement, TimingPass, TimingPassResult};
use crate::CordialResult;
use crate::cli::commands::ActCtx;

/// Full minus omitted time, including the change to shared pipeline work.
#[derive(Debug, Serialize, derive_getters::Getters)]
pub(super) struct TimingDifference {
    stage: String,
    id: String,
    elapsed_nanos: i128,
    full_invocations: u64,
    without_invocations: u64,
}

#[derive(Debug, Serialize, derive_getters::Getters)]
pub(super) struct MarginalPair {
    full: TimingPass,
    without: TimingPass,
    wall_delta_nanos: i128,
    stage_deltas: Vec<TimingDifference>,
}

impl MarginalPair {
    #[instrument(level = "debug", skip(full, without))]
    fn new(full: TimingPass, without: TimingPass) -> Self {
        let full_hooks: BTreeMap<_, _> = full
            .hooks
            .iter()
            .map(|hook| ((hook.stage().as_str(), hook.id().as_str()), hook))
            .collect();
        let without_hooks: BTreeMap<_, _> = without
            .hooks
            .iter()
            .map(|hook| ((hook.stage().as_str(), hook.id().as_str()), hook))
            .collect();
        let keys: BTreeSet<_> = full_hooks
            .keys()
            .chain(without_hooks.keys())
            .copied()
            .collect();
        let stage_deltas = keys
            .into_iter()
            .map(|(stage, id)| {
                let full = full_hooks.get(&(stage, id));
                let without = without_hooks.get(&(stage, id));
                TimingDifference {
                    stage: stage.to_string(),
                    id: id.to_string(),
                    elapsed_nanos: full.map_or(0, |hook| *hook.elapsed_nanos() as i128)
                        - without.map_or(0, |hook| *hook.elapsed_nanos() as i128),
                    full_invocations: full.map_or(0, |hook| *hook.invocations()),
                    without_invocations: without.map_or(0, |hook| *hook.invocations()),
                }
            })
            .collect();
        Self {
            wall_delta_nanos: full.wall_time_nanos as i128 - without.wall_time_nanos as i128,
            full,
            without,
            stage_deltas,
        }
    }
}

#[instrument(level = "debug", skip(ctx), fields(id = id))]
pub(super) fn measure_marginal(
    ctx: &ActCtx,
    id: &str,
    scope: &[String],
) -> CordialResult<MarginalMeasurement> {
    let full_target = tempfile::tempdir()?;
    let without_target = tempfile::tempdir()?;
    let without: Vec<String> = scope
        .iter()
        .filter(|candidate| candidate.as_str() != id)
        .cloned()
        .collect();
    let mut cold = None;
    let mut warm = None;
    let mut error = None;
    for pass in ["cold", "warm"] {
        // Each side keeps its own Cargo target across the cold/warm pair.
        // The pair's full run is repeated for every omitted etiquette.
        let full_result = run_child(ctx, "full", scope, full_target.path(), pass)?;
        let without_result = run_child(
            ctx,
            &format!("without-{id}"),
            &without,
            without_target.path(),
            pass,
        )?;
        match (full_result, without_result) {
            (TimingPassResult::Measured(full), TimingPassResult::Measured(without)) => {
                let pair = MarginalPair::new(full, without);
                if pass == "cold" {
                    cold = Some(pair);
                } else {
                    warm = Some(pair);
                }
            }
            (TimingPassResult::Failed(detail), _) | (_, TimingPassResult::Failed(detail)) => {
                error = Some(format!("{pass}: {detail}"));
                break;
            }
        }
    }
    Ok(MarginalMeasurement {
        scope: scope.to_vec(),
        cold,
        warm,
        cold_overlap_nanos: None,
        warm_overlap_nanos: None,
        error,
    })
}

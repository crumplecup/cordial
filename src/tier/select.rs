//! Resolve declarations, project assignments, and the run's selected tiers.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{CordialConfig, CordialError, CordialResult, Etiquette};
use tracing::instrument;

use super::{Tier, builtin_tier};

/// The validated set of tiers selected for one run.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct TierSelection {
    names: BTreeSet<String>,
}

impl TierSelection {
    /// Resolve `--tier`, or the project's default, against known tier names.
    #[instrument(level = "debug", skip(etiquettes, config))]
    pub fn resolve(
        etiquettes: &[Arc<dyn Etiquette>],
        config: &CordialConfig,
        requested: Option<&[String]>,
    ) -> CordialResult<Self> {
        let mut known: BTreeSet<String> = [Tier::QUICK, Tier::BUILD, Tier::DEEP]
            .into_iter()
            .map(str::to_string)
            .collect();
        known.extend(config.tiers().assign().values().cloned());
        known.extend(
            etiquettes
                .iter()
                .map(|etiquette| etiquette.tier().as_str().to_string()),
        );
        let selected = requested.unwrap_or(config.tiers().default());
        let mut names = if selected.iter().any(|tier| tier == "all") {
            if selected.len() != 1 {
                return Err(CordialError::invariant(
                    "`all` must be the only tier requested",
                ));
            }
            known
        } else {
            for tier in selected {
                if !known.contains(tier) {
                    return Err(CordialError::invariant(format!(
                        "Unknown tier `{tier}`; known tiers: {}",
                        known.iter().cloned().collect::<Vec<_>>().join(", ")
                    )));
                }
            }
            selected.iter().cloned().collect()
        };
        if names.contains(Tier::DEEP) {
            names.insert(Tier::BUILD.to_string());
        }
        if names.contains(Tier::BUILD) {
            names.insert(Tier::QUICK.to_string());
        }
        Ok(Self { names })
    }

    /// Whether the run includes an etiquette's effective tier.
    #[instrument(level = "trace", skip(self, etiquette, config))]
    pub fn includes(&self, etiquette: &dyn Etiquette, config: &CordialConfig) -> bool {
        self.names
            .contains(assigned_tier(etiquette, config).as_str())
    }
}

/// Effective assignment, with project config taking precedence.
#[instrument(level = "debug", skip(etiquette, config))]
pub fn assigned_tier(etiquette: &dyn Etiquette, config: &CordialConfig) -> Tier {
    let name = config
        .tiers()
        .assign()
        .get(etiquette.id())
        .map(String::as_str)
        .or_else(|| builtin_tier(etiquette.id()));
    name.map_or_else(|| etiquette.tier(), Tier::new)
}

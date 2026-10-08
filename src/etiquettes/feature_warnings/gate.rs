//! Suggest the `cfg` gate that silences a feature-dependent warning.

use std::collections::BTreeSet;

use tracing::instrument;

/// One feature combination: the resolved feature set a build ran with.
pub type FeatureSet = BTreeSet<String>;

/// The `cfg` predicate that would make a warning go away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// `#[cfg(any(feature = "a", feature = "b"))]`: the item is used when any
    /// of these is on.
    Any(Vec<String>),
    /// `#[cfg(all(feature = "a", feature = "b"))]`: the item is used only
    /// when all of these are on.
    All(Vec<String>),
    /// No single predicate over features separates the combinations.
    NoSingleGate,
}

impl Gate {
    /// How many features the predicate names (0 for [`Gate::NoSingleGate`]).
    #[instrument(level = "trace", skip(self))]
    pub fn feature_count(&self) -> usize {
        match self {
            Self::Any(features) | Self::All(features) => features.len(),
            Self::NoSingleGate => 0,
        }
    }

    /// The attribute text to paste, or `None` when no single gate fits.
    #[instrument(level = "trace", skip(self))]
    pub fn cfg_expression(&self) -> Option<String> {
        match self {
            Self::Any(features) if features.len() == 1 => Some(feature_pred(&features[0])),
            Self::All(features) if features.len() == 1 => Some(feature_pred(&features[0])),
            Self::Any(features) => Some(format!("any({})", feature_list(features))),
            Self::All(features) => Some(format!("all({})", feature_list(features))),
            Self::NoSingleGate => None,
        }
    }
}

#[instrument(level = "trace")]
fn feature_pred(feature: &str) -> String {
    format!("feature = \"{feature}\"")
}

#[instrument(level = "trace")]
fn feature_list(features: &[String]) -> String {
    features
        .iter()
        .map(|feature| feature_pred(feature))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Suggest a gate from the combinations where a warning fires (`triggering`)
/// and where it does not (`silent`).
///
/// 1. `any`: features that appear in some silent combination and in no
///    triggering one, provided every silent combination has at least one.
/// 2. `all`: features common to every silent combination, provided no
///    triggering combination has them all.
/// 3. Otherwise [`Gate::NoSingleGate`].
#[instrument(level = "debug", skip(triggering, silent))]
pub fn suggest_gate(triggering: &[FeatureSet], silent: &[FeatureSet]) -> Gate {
    if silent.is_empty() {
        return Gate::NoSingleGate;
    }
    let in_silent: FeatureSet = silent.iter().flatten().cloned().collect();
    let in_triggering: FeatureSet = triggering.iter().flatten().cloned().collect();
    let enabling: Vec<String> = in_silent.difference(&in_triggering).cloned().collect();
    if !enabling.is_empty()
        && silent
            .iter()
            .all(|combo| enabling.iter().any(|feature| combo.contains(feature)))
    {
        return Gate::Any(enabling);
    }
    let common = silent.iter().skip(1).fold(silent[0].clone(), |acc, combo| {
        acc.intersection(combo).cloned().collect()
    });
    if !common.is_empty() && !triggering.iter().any(|combo| common.is_subset(combo)) {
        return Gate::All(common.into_iter().collect());
    }
    Gate::NoSingleGate
}

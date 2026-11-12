//! The spread of repeated measurements, in nanoseconds.

use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Repeated samples of one quantity, summarised so two runs can be compared
/// without mistaking noise for change: the median, the extremes and the median
/// absolute deviation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct Spread {
    #[getter(copy)]
    count: u32,
    #[getter(copy)]
    median: u64,
    #[getter(copy)]
    min: u64,
    #[getter(copy)]
    max: u64,
    #[getter(copy)]
    mad: u64,
}

#[instrument(level = "trace", skip(sorted))]
fn median(sorted: &[u64]) -> u64 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        sorted[middle - 1] / 2
            + sorted[middle] / 2
            + (sorted[middle - 1] % 2 + sorted[middle] % 2) / 2
    }
}

impl Spread {
    /// Summarise `samples`; nothing to summarise gives `None`.
    #[instrument(level = "trace", skip(samples))]
    pub fn from_samples(samples: &[u64]) -> Option<Self> {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let (min, max) = (*sorted.first()?, *sorted.last()?);
        let centre = median(&sorted);
        let mut deviations: Vec<u64> = sorted.iter().map(|value| value.abs_diff(centre)).collect();
        deviations.sort_unstable();
        Some(Self {
            count: u32::try_from(sorted.len()).unwrap_or(u32::MAX),
            median: centre,
            min,
            max,
            mad: median(&deviations),
        })
    }

    /// A single measurement, which has no spread to speak of.
    #[instrument(level = "trace")]
    pub fn single(sample: u64) -> Self {
        Self {
            count: 1,
            median: sample,
            min: sample,
            max: sample,
            mad: 0,
        }
    }
}

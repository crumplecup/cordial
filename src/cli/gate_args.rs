//! Shared selection options for commands that run etiquettes.

use clap::Args;
use tracing::instrument;

/// Cost and etiquette selection shared by run, quality, and coverage.
#[derive(Args, Default)]
pub struct GateSelectionArgs {
    /// Select one or more tiers; `all` includes every known tier.
    #[arg(long = "tier", value_delimiter = ',')]
    tiers: Vec<String>,
    /// Run one named etiquette, regardless of its tier.
    #[arg(long)]
    etiquette: Option<String>,
}

impl GateSelectionArgs {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn tiers(&self) -> Option<&[String]> {
        (!self.tiers.is_empty()).then_some(&self.tiers)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn etiquette(&self) -> Option<&str> {
        self.etiquette.as_deref()
    }
}

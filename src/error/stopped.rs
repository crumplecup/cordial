//! Native source for a run the caller asked to end after one pipeline stage.

use std::fmt::{Display, Formatter, Result as FmtResult};

use tracing::instrument;

/// A run that stopped on request, not because something failed.
#[derive(Debug, derive_getters::Getters)]
pub struct StoppedSource {
    stage: String,
}

impl Display for StoppedSource {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "stopped after {}, as asked", self.stage)
    }
}

impl std::error::Error for StoppedSource {}

impl super::CordialError {
    /// The run reached the stage it was asked to stop after.
    #[track_caller]
    #[instrument(level = "debug", skip(stage))]
    #[hotpath::measure]
    pub fn stopped_after(stage: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Stopped(StoppedSource {
            stage: stage.into(),
        }))
    }

    /// The stage this run was stopped after, when it was stopped on request.
    #[instrument(level = "trace", skip(self))]
    pub fn stopped_stage(&self) -> Option<&str> {
        match self.kind() {
            super::CordialErrorKind::Stopped(source) => Some(source.stage()),
            _ => None,
        }
    }
}

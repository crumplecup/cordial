use std::fmt::{Display, Formatter, Result as FmtResult};

use super::anchor::IrAnchor;
use super::artifact::FindingSink;

use tracing::instrument;

/// Identity of a rule that produced a finding.
pub trait Rule: Send + Sync {
    /// Stable rule identifier.
    fn id(&self) -> &str;
    /// Etiquette / lint category this rule belongs to.
    fn category(&self) -> &str;
    /// Human-readable rule description.
    fn description(&self) -> &str;
}

/// A judged issue emitted by an assessor.
pub trait Finding: Send + Sync {
    /// The rule that produced this finding.
    fn rule(&self) -> &dyn Rule;
    /// Open, exemplar, or suppressed.
    fn disposition(&self) -> Disposition;
    /// IR location this item is attached to.
    fn anchor(&self) -> &dyn IrAnchor;
    /// Write structured fields for reporters.
    fn emit(&self, sink: &mut dyn FindingSink);
}

/// Severity or resolution state of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Disposition {
    /// Still unresolved.
    Open,
    /// Kept as a documented example, not a defect.
    Exemplar,
    /// Silenced by an exception patch.
    Suppressed,
}

impl Display for Disposition {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Open => write!(f, "open"),
            Self::Exemplar => write!(f, "exemplar"),
            Self::Suppressed => write!(f, "suppressed"),
        }
    }
}

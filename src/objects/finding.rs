use std::fmt::{Display, Formatter, Result as FmtResult};

use super::anchor::IrAnchor;
use super::artifact::FindingSink;

/// Severity or resolution state of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Disposition {
    Open,
    Exemplar,
    Suppressed,
}

/// Identity of a rule that produced a finding.
pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn category(&self) -> &str;
    fn description(&self) -> &str;
}

/// A judged issue emitted by an assessor.
pub trait Finding: Send + Sync {
    fn rule(&self) -> &dyn Rule;
    fn disposition(&self) -> Disposition;
    fn anchor(&self) -> &dyn IrAnchor;
    fn emit(&self, sink: &mut dyn FindingSink);
}

impl Display for Disposition {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Open => write!(f, "open"),
            Self::Exemplar => write!(f, "exemplar"),
            Self::Suppressed => write!(f, "suppressed"),
        }
    }
}

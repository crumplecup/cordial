//! Trait objects exchanged between hooks and reporters.
//!
//! Cordial keeps observations, judgments, and rendered output separate:
//! [`Marker`] values are probe observations, [`Finding`] values are assessor
//! judgments tied to a [`Rule`], and [`Artifact`] values are reporter output
//! written to the local store.
//!
//! The traits are intentionally small so plugins can supply their own concrete
//! marker, finding, and artifact types without depending on built-in report
//! structs. Anchors and spans provide optional source/IR location context when
//! a finding can point back to code.

pub use anchor::{IrAnchor, NodeAnchor};
pub use artifact::{Artifact, FindingSink, MapFindingSink, TextArtifact};
pub use finding::{Disposition, Finding, Rule};
pub use marker::Marker;
pub use span::{FileSpan, SourceSpan};

mod anchor;
mod artifact;
mod finding;
mod marker;
mod span;

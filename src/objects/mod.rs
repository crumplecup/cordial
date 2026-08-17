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

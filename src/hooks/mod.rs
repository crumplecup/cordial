//! Hook traits used by etiquettes to participate in the session pipeline.
//!
//! Hooks are the executable pieces inside an [`Etiquette`](crate::Etiquette).
//! A loader reads raw material, an enricher mutates the shared IR, a probe
//! records markers, an assessor turns markers into findings, a workspace
//! assessor checks cross-crate state, and a reporter renders artifacts.
//!
//! Each hook method receives a small view struct instead of a long argument
//! list. The view structs carry the session, target IR, and prior-stage data
//! that the hook is allowed to inspect.

mod assessor;
mod enricher;
mod loader;
mod probe;
mod reporter;
mod workspace_assessor;

pub use assessor::{AssessView, Assessor};
pub use enricher::{EnrichView, IrEnricher};
pub use loader::{LoadContext, Loader};
pub use probe::{Probe, ProbeView};
pub use reporter::{RenderView, Reporter};
pub use workspace_assessor::{WorkspaceAssessView, WorkspaceAssessor};

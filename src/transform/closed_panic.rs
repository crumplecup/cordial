//! Close a transitive panic migration using shared written-use evidence.

use crate::error::CordialResult;
use crate::ir::{FileSnapshot, IrView, NodeId, SourceByteRange};
use std::ops::ControlFlow;
use std::path::PathBuf;

use super::{
    PanicComponentBlocker, PanicComponentDecision, PanicComponentPreview,
    plan_owned_panic_component,
};

mod uses;
mod write;
pub use write::{PanicMigrationApplySummary, apply_closed_panic_migration};

/// Result of discharging use and executable-boundary obligations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedPanicDecision {
    /// All written uses are accounted for within the supported binary contract.
    Ready(ClosedPanicMigration),
    /// The selected panic site no longer exists.
    NoCandidate,
    /// No closed plan is exposed when an obligation is unresolved.
    Refused(ClosedPanicBlocker),
}

/// Evidence preventing a closed migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedPanicBlocker {
    /// A declaration, return, or propagation operation could not be composed.
    Component(PanicComponentBlocker),
    /// The boundary is not a supported Cargo unit main.
    UnsupportedEntryPoint,
    /// The Cargo manifest is missing or does not belong to this source root.
    MissingManifestSnapshot,
    /// A source module has no complete use inventory for this snapshot.
    MissingUseInventory(NodeId),
    /// Imports or attributes leave a source namespace unresolved.
    UnresolvedScope(NodeId),
    /// A macro or verbatim island may hide additional references.
    OpaqueSyntax {
        /// Module owning the opaque source range.
        module: NodeId,
        /// Uninterpreted bytes in that module's snapshot.
        range: SourceByteRange,
    },
    /// A matching written name is not a resolved call within the component.
    UnresolvedUse {
        /// Module owning the written reference.
        module: NodeId,
        /// Reference requiring additional resolution or adaptation.
        range: SourceByteRange,
    },
    /// Located calls and the independent written-use inventory disagree.
    IncompleteUseEvidence(NodeId),
}

/// A closed single-module migration with an explicit returned-error boundary.
///
/// Construction requires shared use evidence and Cargo entry identity. This
/// token is distinct from an incomplete preview and cannot be forged through
/// a public constructor. Every closure input remains a write precondition.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ClosedPanicMigration {
    component: PanicComponentPreview,
    inputs: Vec<FileSnapshot>,
    source_root: PathBuf,
}

/// Close a fresh I/O panic migration through the selected unit main.
#[tracing::instrument(level = "debug", skip(ir, leaf, main), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_closed_owned_panic_component(
    ir: &dyn IrView,
    leaf: NodeId,
    site_index: usize,
    main: NodeId,
) -> CordialResult<ClosedPanicDecision> {
    let component = match plan_owned_panic_component(ir, leaf, site_index, main)? {
        PanicComponentDecision::Ready(component) => component,
        PanicComponentDecision::NoCandidate => return Ok(ClosedPanicDecision::NoCandidate),
        PanicComponentDecision::Refused(reason) => {
            return Ok(ClosedPanicDecision::Refused(ClosedPanicBlocker::Component(
                reason,
            )));
        }
    };
    if !super::main_boundary::is_unit_main(ir, main)? {
        return Ok(ClosedPanicDecision::Refused(
            ClosedPanicBlocker::UnsupportedEntryPoint,
        ));
    }
    let mut inputs = match uses::validate(ir, &component)? {
        ControlFlow::Break(blocker) => return Ok(ClosedPanicDecision::Refused(blocker)),
        ControlFlow::Continue(inputs) => inputs,
    };
    let Some(source_root) = component.plan().path().parent() else {
        return Ok(ClosedPanicDecision::Refused(
            ClosedPanicBlocker::MissingManifestSnapshot,
        ));
    };
    let Some(manifest) = ir.cargo_manifest_snapshot()? else {
        return Ok(ClosedPanicDecision::Refused(
            ClosedPanicBlocker::MissingManifestSnapshot,
        ));
    };
    if source_root
        .parent()
        .map(|root| root.join("Cargo.toml"))
        .as_ref()
        != Some(manifest.path())
        || inputs
            .iter()
            .any(|input| !input.path().starts_with(source_root))
    {
        return Ok(ClosedPanicDecision::Refused(
            ClosedPanicBlocker::MissingManifestSnapshot,
        ));
    }
    let source_root = source_root.to_path_buf();
    inputs.push(manifest);
    inputs.sort_by(|left, right| left.path().cmp(right.path()));
    if inputs
        .windows(2)
        .any(|pair| pair[0].path() == pair[1].path())
    {
        return Ok(ClosedPanicDecision::Refused(
            ClosedPanicBlocker::IncompleteUseEvidence(main),
        ));
    }
    Ok(ClosedPanicDecision::Ready(ClosedPanicMigration {
        component,
        inputs,
        source_root,
    }))
}

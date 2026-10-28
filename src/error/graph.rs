//! Kind/Error/wrapper-constructor machinery for IR graph/workspace/loader
//! invariants (`ir/graph.rs`, `ir/workspace.rs`, `ir/crate_load.rs`,
//! `session/resolve.rs`, `hooks/enricher.rs`). The leaf native sources
//! live in `graph_sources.rs`.

use std::panic::Location;

use tracing::instrument;

use super::graph_apply_sources::{MissingConversionUnitSource, StaleSourceSnapshotSource};
use super::graph_sources::{
    CrateMustExistSource, EdgeReferencesMissingNodeSource, NoLoadViewAvailableForEnricherSource,
    NoLoadViewForEnricherSource, SetAttrOnMissingNodeSource, SnapshotEdgeFromInvalidSource,
    SnapshotEdgeMissingEndpointsSource, SnapshotEdgeToInvalidSource,
    SourceEnricherWrongLoaderSource, WorkspaceSourceFactsMissingSource,
};

/// Umbrella kind for IR graph/workspace/loader invariant violations,
/// nested one level under [`super::CordialErrorKind`].
#[derive(Debug)]
pub enum GraphErrorKind {
    EdgeReferencesMissingNode(EdgeReferencesMissingNodeSource),
    SetAttrOnMissingNode(SetAttrOnMissingNodeSource),
    SnapshotEdgeMissingEndpoints(SnapshotEdgeMissingEndpointsSource),
    SnapshotEdgeFromInvalid(SnapshotEdgeFromInvalidSource),
    SnapshotEdgeToInvalid(SnapshotEdgeToInvalidSource),
    CrateMustExist(CrateMustExistSource),
    NoLoadViewForEnricher(NoLoadViewForEnricherSource),
    NoLoadViewAvailableForEnricher(NoLoadViewAvailableForEnricherSource),
    SourceEnricherWrongLoader(SourceEnricherWrongLoaderSource),
    WorkspaceSourceFactsMissing(WorkspaceSourceFactsMissingSource),
    MissingConversionUnit(MissingConversionUnitSource),
    StaleSourceSnapshot(StaleSourceSnapshotSource),
}

#[derive(Debug)]
pub struct GraphError {
    kind: Box<GraphErrorKind>,
    file: String,
    line: u32,
}

impl GraphError {
    #[track_caller]
    #[instrument(level = "debug", skip(kind))]
    fn new(kind: GraphErrorKind) -> Self {
        let loc = Location::caller();
        Self {
            kind: Box::new(kind),
            file: loc.file().to_string(),
            line: loc.line(),
        }
    }
}

impl std::fmt::Display for GraphError {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}:{}] ", self.file, self.line)?;
        match self.kind.as_ref() {
            GraphErrorKind::EdgeReferencesMissingNode(source) => source.fmt(formatter),
            GraphErrorKind::SetAttrOnMissingNode(source) => source.fmt(formatter),
            GraphErrorKind::SnapshotEdgeMissingEndpoints(source) => source.fmt(formatter),
            GraphErrorKind::SnapshotEdgeFromInvalid(source) => source.fmt(formatter),
            GraphErrorKind::SnapshotEdgeToInvalid(source) => source.fmt(formatter),
            GraphErrorKind::CrateMustExist(source) => source.fmt(formatter),
            GraphErrorKind::NoLoadViewForEnricher(source) => source.fmt(formatter),
            GraphErrorKind::NoLoadViewAvailableForEnricher(source) => source.fmt(formatter),
            GraphErrorKind::SourceEnricherWrongLoader(source) => source.fmt(formatter),
            GraphErrorKind::WorkspaceSourceFactsMissing(source) => source.fmt(formatter),
            GraphErrorKind::MissingConversionUnit(source) => source.fmt(formatter),
            GraphErrorKind::StaleSourceSnapshot(source) => source.fmt(formatter),
        }
    }
}

impl std::error::Error for GraphError {
    #[instrument(level = "trace", skip(self))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind.as_ref() {
            GraphErrorKind::EdgeReferencesMissingNode(source) => Some(source),
            GraphErrorKind::SetAttrOnMissingNode(source) => Some(source),
            GraphErrorKind::SnapshotEdgeMissingEndpoints(source) => Some(source),
            GraphErrorKind::SnapshotEdgeFromInvalid(source) => Some(source),
            GraphErrorKind::SnapshotEdgeToInvalid(source) => Some(source),
            GraphErrorKind::CrateMustExist(source) => Some(source),
            GraphErrorKind::NoLoadViewForEnricher(source) => Some(source),
            GraphErrorKind::NoLoadViewAvailableForEnricher(source) => Some(source),
            GraphErrorKind::SourceEnricherWrongLoader(source) => Some(source),
            GraphErrorKind::WorkspaceSourceFactsMissing(source) => Some(source),
            GraphErrorKind::MissingConversionUnit(source) => Some(source),
            GraphErrorKind::StaleSourceSnapshot(source) => Some(source),
        }
    }
}

impl super::CordialError {
    /// A source enricher required facts absent from the workspace IR.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn workspace_source_facts_missing(name: &'static str) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::WorkspaceSourceFactsMissing(WorkspaceSourceFactsMissingSource::new(
                name,
            )),
        )))
    }
    /// An apply plan no longer matches the source file it would overwrite.
    #[track_caller]
    #[instrument(level = "debug", skip(path))]
    pub fn stale_source_snapshot(path: &std::path::Path) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::StaleSourceSnapshot(StaleSourceSnapshotSource::new(path)),
        )))
    }

    /// A conversion candidate was absent from the shared IR.
    #[track_caller]
    #[instrument(level = "debug", skip(path, type_name, field_name))]
    pub fn missing_conversion_unit(
        path: &std::path::Path,
        type_name: &str,
        field_name: &str,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::MissingConversionUnit(MissingConversionUnitSource::new(
                path, type_name, field_name,
            )),
        )))
    }

    /// No load view was found for an enricher on a crate.
    #[track_caller]
    #[instrument(level = "debug", skip(enricher_id, crate_name))]
    pub fn no_load_view_for_enricher(
        enricher_id: impl Into<String>,
        crate_name: impl Into<String>,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::NoLoadViewForEnricher(NoLoadViewForEnricherSource::new(
                enricher_id,
                crate_name,
            )),
        )))
    }

    /// No load view was available for an enricher on a crate.
    #[track_caller]
    #[instrument(level = "debug", skip(enricher_id, crate_name))]
    pub fn no_load_view_available_for_enricher(
        enricher_id: impl Into<String>,
        crate_name: impl Into<String>,
    ) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::NoLoadViewAvailableForEnricher(
                NoLoadViewAvailableForEnricherSource::new(enricher_id, crate_name),
            ),
        )))
    }

    /// A crate must exist in the workspace IR.
    #[track_caller]
    #[instrument(level = "debug", skip(crate_name))]
    pub fn crate_must_exist(crate_name: impl Into<String>) -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::CrateMustExist(CrateMustExistSource::new(crate_name)),
        )))
    }

    /// An edge references a missing graph node.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn edge_references_missing_node() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::EdgeReferencesMissingNode(EdgeReferencesMissingNodeSource::new()),
        )))
    }

    /// `set_attr` was called on a missing graph node.
    #[track_caller]
    #[instrument(level = "trace")]
    pub fn set_attr_on_missing_node() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::SetAttrOnMissingNode(SetAttrOnMissingNodeSource::new()),
        )))
    }

    /// A graph snapshot edge was missing its endpoints.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn snapshot_edge_missing_endpoints() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::SnapshotEdgeMissingEndpoints(SnapshotEdgeMissingEndpointsSource::new()),
        )))
    }

    /// A graph snapshot edge's `from` index was invalid.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn snapshot_edge_from_invalid() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::SnapshotEdgeFromInvalid(SnapshotEdgeFromInvalidSource::new()),
        )))
    }

    /// A graph snapshot edge's `to` index was invalid.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn snapshot_edge_to_invalid() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::SnapshotEdgeToInvalid(SnapshotEdgeToInvalidSource::new()),
        )))
    }

    /// A source enricher received a loader view of the wrong kind.
    #[track_caller]
    #[instrument(level = "debug")]
    pub fn source_enricher_wrong_loader() -> Self {
        Self::from_kind(super::CordialErrorKind::Graph(GraphError::new(
            GraphErrorKind::SourceEnricherWrongLoader(SourceEnricherWrongLoaderSource::new()),
        )))
    }
}

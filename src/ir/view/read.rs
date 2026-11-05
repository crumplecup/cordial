//! Read-only graph and typed evidence interface for pipeline consumers.

use super::evidence;
use crate::error::CordialResult;
use crate::ir::{ATTR_CRATE_KINDS, CrateKind, EdgeKind, NodeId, NodeRef, Query};
use tracing::instrument;

/// Read-only view over one crate's IR graph.
///
/// Probes, assessors, and reporters use this trait instead of depending on the
/// concrete petgraph storage. All node ids are local to this crate graph and
/// remain valid only for the IR snapshot passed to the hook.
pub trait IrView {
    /// Source input identity for the semantic projection in this graph.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn semantic_snapshot(&self) -> CordialResult<Option<crate::ir::SemanticSnapshot>> {
        evidence::optional(self, self.root()?, crate::ir::ATTR_SEMANTIC_SNAPSHOT)
    }

    /// Read the source database's snapshot-local semantic function facts.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn semantic_function(&self, id: NodeId) -> CordialResult<Option<crate::ir::SemanticFunction>> {
        evidence::optional(self, id, crate::ir::ATTR_SEMANTIC_FUNCTION)
    }

    /// Read signature-level or prospective migration alternatives from shared IR.
    /// Missing facts never establish an empty error family.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn function_error_family(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<crate::ir::FunctionErrorFamily>> {
        evidence::optional(self, id, crate::ir::ATTR_FUNCTION_ERROR_FAMILY)
    }

    /// Read the manifest identity observed during Cargo target discovery.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn cargo_manifest_snapshot(&self) -> CordialResult<Option<crate::ir::FileSnapshot>> {
        evidence::optional(self, self.root()?, crate::ir::ATTR_CARGO_MANIFEST_SNAPSHOT)
    }

    /// Read Cargo's binary entry paths; missing metadata cannot prove a boundary.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn binary_entry_paths(&self) -> CordialResult<Option<Vec<std::path::PathBuf>>> {
        evidence::optional(self, self.root()?, crate::ir::ATTR_BINARY_ENTRY_PATHS)
    }

    /// Read persisted call resolution; missing evidence never means no callers.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn function_call_flow(&self, id: NodeId) -> CordialResult<Option<crate::ir::FunctionCallFlow>> {
        evidence::optional(self, id, crate::ir::ATTR_FUNCTION_CALL_FLOW)
    }

    /// Read complete loader-owned file-policy evidence, including inactive syntax.
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_policy_inventory(&self) -> CordialResult<Option<crate::ir::SourcePolicyInventory>> {
        evidence::optional(self, self.root()?, crate::ir::ATTR_SOURCE_POLICY_INVENTORY)
    }

    /// Read file attribute evidence collected by the source loader.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_file_attributes(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<crate::ir::SourceFileAttributes>> {
        evidence::optional(self, id, crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES)
    }

    /// Read loader-projected tracing declarations without accessing source syntax.
    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_tracing_inventory(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<crate::ir::SourceTracingInventory>> {
        evidence::optional(self, id, crate::ir::ATTR_SOURCE_TRACING_INVENTORY)
    }

    /// Read required architecture contracts without source or private caches.
    #[cfg(feature = "internal_error_chain")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_architecture(&self) -> CordialResult<crate::ir::SourceErrorArchitecture> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_ARCHITECTURE,
            "source error-architecture inventory",
        )
    }

    /// Read required conversion signatures without source or private caches.
    #[cfg(feature = "internal_error_chain")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_signatures(&self) -> CordialResult<crate::ir::SourceErrorSignatures> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_SIGNATURES,
            "source error-signature inventory",
        )
    }

    /// Read required compliance evidence without syntax or private caches.
    #[cfg(feature = "internal_error_chain")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_compliance(&self) -> CordialResult<crate::ir::SourceErrorCompliance> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_COMPLIANCE,
            "source error-compliance inventory",
        )
    }

    /// Read required type relationships without syntax or private caches.
    #[cfg(feature = "internal_error_chain")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_types(&self) -> CordialResult<crate::ir::SourceErrorTypes> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_TYPES,
            "source error-type inventory",
        )
    }

    /// Read required chain evidence without source syntax or private caches.
    #[cfg(feature = "error_chain")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_chains(&self) -> CordialResult<crate::ir::SourceErrorChains> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_CHAINS,
            "source error-chain inventory",
        )
    }

    /// Read required error-site evidence without accessing source syntax.
    #[cfg(feature = "error_sites")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_error_sites(&self) -> CordialResult<crate::ir::SourceErrorSites> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_ERROR_SITES,
            "source error-site inventory",
        )
    }

    /// Read required crate-wide tracing evidence; malformed evidence is an error.
    #[cfg(feature = "tracing")]
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_observability(&self) -> CordialResult<crate::ir::SourceObservability> {
        evidence::required(
            self,
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            "source observability inventory",
        )
    }

    /// Read the located syntax attached by the shared attribute enricher.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_attribute(&self, id: NodeId) -> CordialResult<Option<crate::ir::SourceAttribute>> {
        evidence::optional(self, id, crate::ir::ATTR_SOURCE_ATTRIBUTE)
    }

    /// Read namespace evidence collected by the source loader.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_module_scope(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<crate::ir::SourceModuleScope>> {
        evidence::optional(self, id, crate::ir::ATTR_SOURCE_MODULE_SCOPE)
    }

    /// Read declared function syntax without reparsing source or hiding corrupt facts.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn function_source_syntax(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<crate::ir::FunctionSourceSyntax>> {
        evidence::optional(self, id, crate::ir::ATTR_FUNCTION_SOURCE_SYNTAX)
    }

    /// Cargo package name this crate IR belongs to.
    fn crate_name(&self) -> &str;
    /// Dependency survey facts collected once for the current workspace run.
    #[cfg(feature = "dependency_freshness")]
    fn workspace_dependency_freshness_records(
        &self,
    ) -> Option<&[crate::etiquettes::DependencySurveyRecord]> {
        None
    }
    /// Merged std/core/alloc inventory retained for the current workspace run.
    #[cfg(feature = "homecoming_std")]
    fn workspace_framework_std_inventory(
        &self,
    ) -> Option<&[crate::framework_std::StdInventoryItem]> {
        None
    }
    /// Proof harness facts retained for the current workspace run.
    #[cfg(any(feature = "impl_coverage", feature = "amenable_std"))]
    fn workspace_proof_harness(
        &self,
    ) -> Option<std::sync::Arc<crate::proof_harness::ProofHarness>> {
        None
    }
    /// Required, snapshot-bound Verus dialect evidence on the shared graph.
    #[cfg(feature = "verus_ir")]
    fn source_verus(&self) -> CordialResult<crate::ir::SourceVerus> {
        evidence::required(self, crate::ir::ATTR_SOURCE_VERUS, "source Verus inventory")
    }
    /// Workspace antipattern evidence collected once per run.
    #[cfg(feature = "antipatterns")]
    fn workspace_antipattern_facts(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::antipatterns::AntipatternWorkspaceFacts>> {
        None
    }
    /// Workspace path-splice and dependency facts, when collected.
    #[cfg(feature = "derives")]
    fn workspace_path_inclusions(&self) -> Option<std::sync::Arc<crate::PathInclusionFacts>> {
        None
    }
    /// Workspace proof-only call reachability, when collected.
    #[cfg(feature = "tracing")]
    fn workspace_call_graph(
        &self,
    ) -> Option<std::sync::Arc<crate::etiquettes::tracing::CallGraphFacts>> {
        None
    }
    /// Root node of this crate graph.
    fn root(&self) -> CordialResult<NodeId>;
    /// Borrow the node with this id, if it exists in this crate graph.
    fn node(&self, id: NodeId) -> Option<NodeRef<'_>>;
    /// Return nodes whose kinds and attributes match `query`.
    ///
    /// The default concrete implementation uses [`Query::node_kinds`] as a
    /// cheap prefilter, then calls [`Query::matches_node`] for the final
    /// predicate.
    fn nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>>;
    /// Matching nodes across all crate graphs in this workspace.
    /// Standalone crate views return their own matching nodes.
    fn workspace_nodes_matching(&self, query: &dyn Query) -> Vec<NodeRef<'_>> {
        self.nodes_matching(query)
    }
    /// Parent node ids along incoming edges of `kind`.
    fn parents(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    /// Child node ids along outgoing edges of `kind`.
    fn children(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId>;
    /// Node id for a `foo::bar` path, if the path index knows it.
    fn node_by_path(&self, path: &str) -> Option<NodeId>;

    /// All declarations at a path; a qualified name alone is not an identity.
    #[instrument(level = "trace", skip(self))]
    fn nodes_by_path(&self, path: &str) -> Vec<NodeId> {
        self.nodes_matching(
            &crate::ir::QueryBuilder::new()
                .with_attr("qualified_path", path)
                .build(),
        )
        .into_iter()
        .map(|node| node.id())
        .collect()
    }

    /// Join source evidence to its declaration, never to a same-named sibling.
    /// Ambiguous locations require more evidence rather than an arbitrary pick.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn source_node_at(
        &self,
        path: &str,
        file: &std::path::Path,
        line: u32,
    ) -> CordialResult<Option<NodeId>> {
        use crate::objects::SourceSpan;
        let mut matching = self.nodes_by_path(path).into_iter().filter(|id| {
            self.node(*id).is_some_and(|node| {
                node.weight()
                    .span()
                    .as_ref()
                    .is_some_and(|span| span.file() == file && span.line() == line)
            })
        });
        let first = matching.next();
        if matching.next().is_some() {
            return Err(crate::CordialError::workspace_source_facts_missing(
                "unique source declaration",
            ));
        }
        Ok(first)
    }

    /// Cargo kinds recorded on the crate root, empty when unknown.
    ///
    /// Empty means the loader had no Cargo metadata (a synthetic target or a
    /// cached snapshot from before kinds were recorded), not that the crate
    /// is a plain library.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn crate_kinds(&self) -> CordialResult<Vec<CrateKind>> {
        let root = self.root()?;
        Ok(self
            .node(root)
            .and_then(|node| {
                node.attr(ATTR_CRATE_KINDS)
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().and_then(CrateKind::from_attr))
                            .collect()
                    })
            })
            .unwrap_or_default())
    }

    /// Whether this crate is a `proc-macro = true` library.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn is_proc_macro(&self) -> CordialResult<bool> {
        Ok(self.crate_kinds()?.contains(&CrateKind::ProcMacro))
    }
}

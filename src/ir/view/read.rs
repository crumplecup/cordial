//! Read-only graph and typed evidence interface for pipeline consumers.

use super::evidence;

#[cfg(feature = "internal_error_chain")]
mod error;
mod source;
mod workspace;
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

    /// Read function facts in every non-ordinary context that resolved it.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn semantic_context_functions(
        &self,
        id: NodeId,
    ) -> CordialResult<Vec<crate::ir::SemanticContextFunction>> {
        Ok(
            evidence::optional(self, id, crate::ir::ATTR_SEMANTIC_CONTEXT_FUNCTIONS)?
                .unwrap_or_default(),
        )
    }

    /// The non-ordinary contexts analysed for this crate, if a projection ran.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn semantic_context_plan(&self) -> CordialResult<Option<crate::ir::SemanticContextPlan>> {
        evidence::optional(self, self.root()?, crate::ir::ATTR_SEMANTIC_CONTEXT_PLAN)
    }

    /// Contexts in which this function is active, or `None` when no semantic
    /// projection ran. `Some(vec![])` means every analysed context leaves it
    /// inactive: absence of facts is never silently "no calls". Functions
    /// declared inside another function's body are active only in the ordinary
    /// context; their enclosing function's `incomplete` flag marks that gap.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    fn active_contexts(
        &self,
        id: NodeId,
    ) -> CordialResult<Option<Vec<crate::ir::SemanticContextKey>>> {
        if self.semantic_snapshot()?.is_none() {
            return Ok(None);
        }
        let mut active = Vec::new();
        if self.semantic_function(id)?.is_some() {
            active.push(crate::ir::SemanticContextKey::ordinary());
        }
        if let (Some(plan), Some(indexes)) = (
            self.semantic_context_plan()?,
            evidence::optional::<Vec<u32>>(self, id, crate::ir::ATTR_SEMANTIC_ACTIVITY)?,
        ) {
            active.extend(
                indexes
                    .into_iter()
                    .filter_map(|index| plan.contexts().get(index as usize).cloned()),
            );
        }
        Ok(Some(active))
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

    source::readers!();

    #[cfg(feature = "internal_error_chain")]
    error::required_readers!();

    /// Cargo package name this crate IR belongs to.
    fn crate_name(&self) -> &str;
    workspace::readers!();

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
    /// Callers resolved in `context`, along incoming `ContextCalls` edges.
    /// The ordinary context has none here: read it through `Calls`.
    fn context_parents(&self, id: NodeId, context: &crate::ir::SemanticContextKey) -> Vec<NodeId>;
    /// Callees resolved in `context`, along outgoing `ContextCalls` edges.
    fn context_children(&self, id: NodeId, context: &crate::ir::SemanticContextKey) -> Vec<NodeId>;
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

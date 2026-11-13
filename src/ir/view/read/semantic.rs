//! Semantic evidence interfaces inserted into the public IrView trait: function
//! facts, cfg contexts, locations, cross-crate calls, trait implementations and
//! macro-read functions.

macro_rules! readers {
    () => {
        /// Source input identity for the semantic projection in this graph.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn semantic_snapshot(&self) -> CordialResult<Option<crate::ir::SemanticSnapshot>> {
            evidence::optional(self, self.root()?, crate::ir::ATTR_SEMANTIC_SNAPSHOT)
        }

        /// Read the source database's snapshot-local semantic function facts.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn semantic_function(
            &self,
            id: NodeId,
        ) -> CordialResult<Option<crate::ir::SemanticFunction>> {
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

        /// The functions of leaf crates (integration-test binaries), by location.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        fn leaf_functions(&self) -> CordialResult<Vec<crate::ir::LeafFunction>> {
            Ok(
                evidence::optional(self, self.root()?, crate::ir::ATTR_LEAF_FUNCTIONS)?
                    .unwrap_or_default(),
            )
        }

        /// Where this function item starts in source, if the loader recorded it.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn function_location(
            &self,
            id: NodeId,
        ) -> CordialResult<Option<crate::ir::FunctionLocation>> {
            evidence::optional(self, id, crate::ir::ATTR_FUNCTION_LOCATION)
        }

        /// The trait method this function implements, if it implements one.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn trait_implementation(
            &self,
            id: NodeId,
        ) -> CordialResult<Option<crate::ir::TraitImplementation>> {
            evidence::optional(self, id, crate::ir::ATTR_TRAIT_IMPLEMENTATION)
        }

        /// The macro-invocation facts of a function read from a macro's tokens.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn macro_island(&self, id: NodeId) -> CordialResult<Option<crate::ir::MacroIsland>> {
            evidence::optional(self, id, crate::ir::ATTR_MACRO_ISLAND)
        }

        /// Calls this function makes into other workspace crates, in every context.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn external_calls(&self, id: NodeId) -> CordialResult<Vec<crate::ir::ExternalCall>> {
            Ok(evidence::optional(self, id, crate::ir::ATTR_EXTERNAL_CALLS)?.unwrap_or_default())
        }

        /// The non-ordinary contexts analysed for this crate, if a projection ran.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
        fn semantic_context_plan(&self) -> CordialResult<Option<crate::ir::SemanticContextPlan>> {
            evidence::optional(self, self.root()?, crate::ir::ATTR_SEMANTIC_CONTEXT_PLAN)
        }

        /// Contexts in which this function is active, or `None` when no semantic
        /// projection ran. `Some(vec![])` means every analysed context leaves it
        /// inactive: absence of facts is never silently "no calls". Functions
        /// declared inside another function's body are active only in the ordinary
        /// context; their enclosing function's `incomplete` flag marks that gap.
        #[instrument(level = "trace", skip(self), err(level = "warn"))]
        #[hotpath::measure]
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
    };
}

pub(super) use readers;

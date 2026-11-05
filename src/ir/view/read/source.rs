//! Loader-owned source evidence readers, inserted into the public IrView trait.

macro_rules! readers {
    () => {
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
        fn source_policy_inventory(
            &self,
        ) -> CordialResult<Option<crate::ir::SourcePolicyInventory>> {
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
        fn source_attribute(
            &self,
            id: NodeId,
        ) -> CordialResult<Option<crate::ir::SourceAttribute>> {
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

        /// Required, snapshot-bound panic and Kani proof evidence.
        #[cfg(feature = "panics")]
        fn source_panics(&self) -> CordialResult<crate::ir::SourcePanics> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_PANICS,
                "source panic inventory",
            )
        }
        /// Required, snapshot-bound Verus dialect evidence on the shared graph.
        #[cfg(feature = "verus_ir")]
        fn source_verus(&self) -> CordialResult<crate::ir::SourceVerus> {
            evidence::required(self, crate::ir::ATTR_SOURCE_VERUS, "source Verus inventory")
        }
        /// Required, snapshot-bound antipattern evidence independent of policy.
        #[cfg(feature = "antipatterns")]
        fn source_antipatterns(&self) -> CordialResult<crate::ir::SourceAntipatterns> {
            evidence::required(
                self,
                crate::ir::ATTR_SOURCE_ANTIPATTERNS,
                "source antipattern inventory",
            )
        }
    };
}

pub(super) use readers;

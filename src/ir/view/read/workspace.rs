//! Workspace evidence interfaces inserted into the public IrView trait.

macro_rules! readers {
    () => {
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
    };
}

pub(super) use readers;

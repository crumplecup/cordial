//! Project canonical database source and semantic facts into shared IR.

#[cfg(feature = "antipatterns")]
mod antipatterns;
#[cfg(feature = "cli_layout")]
mod cli_layout;
mod declarations;
#[cfg(feature = "internal_error_chain")]
mod error_apply;
#[cfg(feature = "internal_error_chain")]
mod error_architecture;
#[cfg(feature = "error_chain")]
mod error_chains;
#[cfg(feature = "internal_error_chain")]
mod error_compliance;
#[cfg(feature = "internal_error_chain")]
mod error_signatures;
#[cfg(feature = "error_sites")]
mod error_sites;
#[cfg(feature = "internal_error_chain")]
mod error_types;
#[cfg(feature = "tracing")]
mod observability;
#[cfg(feature = "panics")]
mod panics;
#[cfg(feature = "derives")]
mod path_inclusion;
mod selection;
#[cfg(feature = "verus_ir")]
mod verus;
#[cfg(feature = "visibility")]
mod visibility;

use super::SourceDatabase;
use crate::{CordialResult, CrateIr};
use ra_ap_hir::attach_db;
use std::path::PathBuf;
use tracing::instrument;

impl SourceDatabase {
    /// Materialize tracked semantic facts into the ordinary shared graph.
    /// Source paths belong to the target's loaded Cargo configuration. Use
    /// `project_target` to collect package scan roots and declared entry files.
    #[instrument(level = "debug", skip(self, paths, target), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project(
        &mut self,
        target: &crate::CrateTarget,
        paths: &[PathBuf],
    ) -> CordialResult<CrateIr> {
        self.project_selection(target, paths, paths)
    }

    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn project_selection(
        &mut self,
        target: &crate::CrateTarget,
        paths: &[PathBuf],
        policy_paths: &[PathBuf],
    ) -> CordialResult<CrateIr> {
        let context = self.projection_context()?;
        selection::validate_target(target, &context)?;
        #[cfg(feature = "cfg_hygiene")]
        let declarations = {
            let workspace_root = self
                .workspace_root
                .clone()
                .unwrap_or_else(|| target.crate_root().clone());
            let mut inventory = crate::ir::SourcePolicyInventory::default();
            inventory.collect_declarations(target.crate_root(), &workspace_root, |path| {
                self.input_text(path)
            })?;
            inventory
        };
        let ordinary_watch = crate::process_memory::Watch::start("ordinary projection".to_owned());
        let (mut graph, outline) = attach_db(&self.db, || {
            self.project_attached(
                target,
                paths,
                policy_paths,
                #[cfg(feature = "cfg_hygiene")]
                declarations,
            )
        })?;
        drop(ordinary_watch);
        crate::process_memory::log("ordinary projection done");
        // Gated code is analysed by changing the database's cfg input, which
        // needs the exclusive borrow the ordinary pass does not have.
        self.project_contexts(paths, outline.functions(), &mut graph)?;
        #[cfg(feature = "tracing")]
        self.project_tracing_inventories(&outline, &mut graph)?;
        Ok(graph)
    }

    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn project_attached(
        &self,
        target: &crate::CrateTarget,
        paths: &[PathBuf],
        policy_paths: &[PathBuf],
        #[cfg(feature = "cfg_hygiene")] declarations: crate::ir::SourcePolicyInventory,
    ) -> CordialResult<(CrateIr, declarations::Outline)> {
        let mut graph = CrateIr::new(target.crate_name());
        selection::target_attributes(target, &mut graph)?;
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
        let mut policies = {
            #[cfg(feature = "cfg_hygiene")]
            {
                declarations
            }
            #[cfg(not(feature = "cfg_hygiene"))]
            {
                crate::ir::SourcePolicyInventory::default()
            }
        };
        #[cfg(feature = "visibility")]
        policies.set_visibility(
            target.crate_root(),
            self.project_visibility(target.crate_root())?,
        );
        #[cfg(feature = "cli_layout")]
        policies.set_cli_layout(
            target.crate_root(),
            self.project_cli_layout(target.crate_root(), target.crate_name())?,
        );
        #[cfg(feature = "tracing")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            serde_json::to_value(self.project_observability(target.crate_root())?)?,
        )?;
        #[cfg(feature = "verus_ir")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_VERUS,
            serde_json::to_value(self.project_verus(target.crate_root())?)?,
        )?;
        #[cfg(feature = "panics")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_PANICS,
            serde_json::to_value(self.project_panics(
                target.crate_root(),
                #[cfg(feature = "verus_ir")]
                &crate::IrView::source_verus(&graph)?.facts(),
            )?)?,
        )?;
        #[cfg(feature = "antipatterns")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ANTIPATTERNS,
            serde_json::to_value(self.project_antipatterns(target)?)?,
        )?;
        #[cfg(feature = "error_sites")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_SITES,
            serde_json::to_value(self.project_error_sites(target.crate_root())?)?,
        )?;
        #[cfg(feature = "error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_CHAINS,
            serde_json::to_value(self.project_error_chains(target.crate_root())?)?,
        )?;
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_TYPES,
            serde_json::to_value(self.project_error_types(target.crate_root())?)?,
        )?;
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_COMPLIANCE,
            serde_json::to_value(self.project_error_compliance(target)?)?,
        )?;
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_SIGNATURES,
            serde_json::to_value(self.project_error_signatures(target.crate_root())?)?,
        )?;
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_ARCHITECTURE,
            serde_json::to_value(self.project_error_architecture(target)?)?,
        )?;
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_APPLY,
            serde_json::to_value(self.project_error_apply(target.crate_root())?)?,
        )?;
        let (snapshots, outline) =
            self.project_declarations(target.crate_root(), paths, &mut graph)?;
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
        let snapshots = {
            let mut snapshots = snapshots;
            let sema = ra_ap_hir::Semantics::new(&self.db);
            for path in policy_paths {
                let source = self.policy_source(path, &sema)?;
                policies.collect_native_file(&source, target.crate_root())?;
                if !snapshots
                    .iter()
                    .any(|snapshot| snapshot.path() == source.path())
                {
                    snapshots.push(crate::FileSnapshot::new(
                        source.path().to_path_buf(),
                        source.source_digest(),
                    ));
                }
            }
            policies.sort();
            graph.set_attr(
                graph.root(),
                crate::ir::ATTR_SOURCE_POLICY_INVENTORY,
                serde_json::to_value(policies)?,
            )?;
            snapshots
        };
        #[cfg(not(any(
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
        )))]
        let _ = policy_paths;
        graph.set_attr(
            graph.root(),
            crate::ATTR_SEMANTIC_SNAPSHOT,
            serde_json::to_value(crate::SemanticSnapshot::new(
                self.revision,
                snapshots,
                self.projection_context()?,
            ))?,
        )?;
        Ok((graph, outline))
    }
}

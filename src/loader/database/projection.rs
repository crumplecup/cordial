//! Project canonical database source and semantic facts into shared IR.

#[cfg(feature = "antipatterns")]
mod antipatterns;
#[cfg(feature = "cli_layout")]
mod cli_layout;
mod declarations;
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
use super::scan_queries;
pub(super) use declarations::source_of;
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
        super::queries::forget_computed();
        self.declare_cutoffs();
        if self.restored {
            self.warm_persisted_dependencies(paths);
            self.warm_fingerprint();
        }
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
        self.prime_function_evidence();
        self.audit_reused_evidence();
        self.itemize_memory("evidence");
        self.evict_regenerable("evidence");
        #[cfg(any(
            feature = "error_sites",
            feature = "error_chain",
            feature = "internal_error_chain",
            feature = "tracing",
            feature = "verus_ir",
            feature = "visibility",
            feature = "allows",
            feature = "cfg_hygiene",
            feature = "cfg_scatter",
            feature = "glob_imports",
            feature = "inline_tests",
            feature = "modularity",
            feature = "pageantry",
            feature = "derives",
            feature = "cli_layout"
        ))]
        self.prime_file_scans(target)?;
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
        self.evict_regenerable("ordinary projection");
        if crate::memory_limit::pressured() {
            // What the steps read of the graph's bulk attributes goes back to bytes.
            let dropped = graph.drop_decoded_attrs();
            tracing::info!(
                target: "cordial::eviction",
                step = "ordinary projection",
                dropped_decoded = dropped,
                "the graph's bulk attributes that were read are bytes again"
            );
        }
        self.log_tables();
        // Gated code is analysed by changing the database's cfg input, which
        // needs the exclusive borrow the ordinary pass does not have.
        let rest = self.project_after_ordinary(paths, &outline, &mut graph);
        // A run that fails in the contexts still leaves what it computed.
        self.save_state();
        rest?;
        // What the database now holds, for a profile; it needs the database
        // exclusively, which it has here.
        #[cfg(feature = "hotpath")]
        crate::storage::record_database(&mut self.db);
        Ok(graph)
    }

    /// What follows the ordinary pass: the cfg contexts, then (with tracing) the inventories,
    /// the leaf functions and the observability calls, which read what the contexts found.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn project_after_ordinary(
        &mut self,
        paths: &[PathBuf],
        outline: &declarations::Outline,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        // Gated code is analysed by changing the database's cfg input, which
        // needs the exclusive borrow the ordinary pass does not have.
        self.project_contexts(paths, outline.functions(), graph)?;
        #[cfg(feature = "tracing")]
        {
            self.prime_tracing_inventories(outline);
            self.project_tracing_inventories(outline, graph)?;
            self.project_leaves(graph)?;
            self.project_observability_calls(outline, graph)?;
        }
        Ok(())
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
            for path in policy_paths {
                let key = scan_queries::ScanKey::new(
                    &self.db,
                    self.file_id(path)?,
                    path.clone(),
                    PathBuf::new(),
                    target.crate_root().to_path_buf(),
                    String::new(),
                    false,
                );
                let (part, snapshot) = match scan_queries::policy_file(&self.db, key) {
                    Some(found) => (found.part().clone(), found.snapshot().clone()),
                    // The query declines a file that does not parse; the direct
                    // path reports why.
                    None => {
                        let sema = ra_ap_hir::Semantics::new(&self.db);
                        let source = self.policy_source(path, &sema)?;
                        (
                            crate::ir::SourcePolicyInventory::native_file_part(
                                &source,
                                target.crate_root(),
                            )?,
                            crate::FileSnapshot::new(
                                source.path().to_path_buf(),
                                source.source_digest(),
                            ),
                        )
                    }
                };
                policies.absorb(part);
                if !snapshots
                    .iter()
                    .any(|existing| existing.path() == snapshot.path())
                {
                    snapshots.push(snapshot);
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

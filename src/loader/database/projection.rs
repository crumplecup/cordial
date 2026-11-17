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
        self.evict_regenerable("evidence");
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
        self.report_heap("before visibility step a");
        #[cfg(feature = "visibility")]
        policies.set_visibility(
            target.crate_root(),
            self.project_visibility(target.crate_root())?,
        );
        self.report_heap("before cli_layout step b");
        #[cfg(feature = "cli_layout")]
        policies.set_cli_layout(
            target.crate_root(),
            self.project_cli_layout(target.crate_root(), target.crate_name())?,
        );
        self.report_heap("before tracing step c");
        #[cfg(feature = "tracing")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            serde_json::to_value(self.project_observability(target.crate_root())?)?,
        )?;
        self.report_heap("before verus_ir step d");
        #[cfg(feature = "verus_ir")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_VERUS,
            serde_json::to_value(self.project_verus(target.crate_root())?)?,
        )?;
        self.report_heap("before panics step e");
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
        self.report_heap("before antipatterns step f");
        #[cfg(feature = "antipatterns")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ANTIPATTERNS,
            serde_json::to_value(self.project_antipatterns(target)?)?,
        )?;
        self.report_heap("before error_sites step g");
        #[cfg(feature = "error_sites")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_SITES,
            serde_json::to_value(self.project_error_sites(target.crate_root())?)?,
        )?;
        self.report_heap("before error_chain step h");
        #[cfg(feature = "error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_CHAINS,
            serde_json::to_value(self.project_error_chains(target.crate_root())?)?,
        )?;
        self.report_heap("before internal_error_chain step i");
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_TYPES,
            serde_json::to_value(self.project_error_types(target.crate_root())?)?,
        )?;
        self.report_heap("before internal_error_chain step j");
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_COMPLIANCE,
            serde_json::to_value(self.project_error_compliance(target)?)?,
        )?;
        self.report_heap("before internal_error_chain step k");
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_SIGNATURES,
            serde_json::to_value(self.project_error_signatures(target.crate_root())?)?,
        )?;
        self.report_heap("before internal_error_chain step l");
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_ARCHITECTURE,
            serde_json::to_value(self.project_error_architecture(target)?)?,
        )?;
        self.report_heap("before internal_error_chain step m");
        #[cfg(feature = "internal_error_chain")]
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_ERROR_APPLY,
            serde_json::to_value(self.project_error_apply(target.crate_root())?)?,
        )?;
        self.report_heap("before declarations step n");
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

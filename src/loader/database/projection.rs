//! Project canonical database source and semantic facts into shared IR.

#[cfg(feature = "cli_layout")]
mod cli_layout;
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
#[cfg(feature = "derives")]
mod path_inclusion;
#[cfg(feature = "verus_ir")]
mod verus;
#[cfg(feature = "visibility")]
mod visibility;

use super::SourceDatabase;
use crate::loader::database::{DatabaseSource, populate::Projection};
use crate::{CordialResult, CrateIr};
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tracing::instrument;

impl SourceDatabase {
    /// Materialize tracked semantic facts into the ordinary shared graph.
    /// The source selection must belong to one library target and Cargo
    /// configuration. Broader target contexts are part of loader migration.
    #[instrument(level = "debug", skip(self, paths, target), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project(
        &mut self,
        target: &crate::CrateTarget,
        paths: &[PathBuf],
    ) -> CordialResult<CrateIr> {
        self.projection_context()?;
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
        attach_db(&self.db, || {
            self.project_attached(
                target,
                paths,
                #[cfg(feature = "cfg_hygiene")]
                declarations,
            )
        })
    }

    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    fn project_attached(
        &self,
        target: &crate::CrateTarget,
        paths: &[PathBuf],
        #[cfg(feature = "cfg_hygiene")] declarations: crate::ir::SourcePolicyInventory,
    ) -> CordialResult<CrateIr> {
        let mut graph = CrateIr::new(target.crate_name());
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
        let mut snapshots = Vec::new();
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
        let mut projection = Projection::default();
        let sema = Semantics::new(&self.db);
        for path in paths {
            let file = self.file_id(path)?;
            let text = self.db.file_text(file).text(&self.db);
            snapshots.push(crate::ir::FileSnapshot::new(
                path.canonicalize()?,
                Sha256::digest(text.as_bytes()).into(),
            ));
            let edition = sema.attach_first_edition(file);
            // This consults the same memoized parse used by Semantics. Parser
            // recovery must not turn invalid syntax into complete edit facts.
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                #[cfg(any(
                    feature = "tracing",
                    feature = "internal_error_chain",
                    feature = "verus_ir"
                ))]
                &self.db,
                path,
                text,
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            let module_path = sema
                .file_to_module_defs(file)
                .next()
                .map(|module| {
                    module
                        .path_to_root(&self.db)
                        .into_iter()
                        .rev()
                        .filter_map(|module| module.name(&self.db))
                        .map(|name| name.as_str().to_owned())
                        .collect::<Vec<_>>()
                        .join("::")
                })
                .unwrap_or_else(|| path.display().to_string());
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
            policies.collect_native_file(&source, target.crate_root())?;
            projection.populate(&self.db, &sema, &source, &module_path, &mut graph)?;
        }
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
        {
            policies.sort();
            graph.set_attr(
                graph.root(),
                crate::ir::ATTR_SOURCE_POLICY_INVENTORY,
                serde_json::to_value(policies)?,
            )?;
        }
        projection.connect(&mut graph)?;
        graph.set_attr(
            graph.root(),
            crate::ATTR_SEMANTIC_SNAPSHOT,
            serde_json::to_value(crate::SemanticSnapshot::new(
                self.revision,
                snapshots,
                self.projection_context()?,
            ))?,
        )?;
        Ok(graph)
    }
}

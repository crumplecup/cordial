//! Serialize crate-wide error declaration evidence into the shared IR.

use std::path::PathBuf;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::ir::{IrMut, IrView};

use super::super::edges::ErrorEdgeTarget;
use super::super::syntax_facts::FileSyntaxFacts;
use super::CrateTypeIndex;
use super::paths::ModuleIdentity;

const TYPE_INDEX_ATTR: &str = "error_apply_type_index";

/// Sequence entries keep path and position keys valid JSON without
/// stringifying identities or parsing source again when the planner reads them.
#[derive(serde::Serialize, serde::Deserialize)]
struct CrateTypeIndexFacts {
    declarations: Vec<(Vec<String>, Vec<Option<ErrorEdgeTarget>>)>,
    local: Vec<(PathBuf, String, Vec<Option<ErrorEdgeTarget>>)>,
    modules: Vec<(PathBuf, ModuleIdentity)>,
    imports: Vec<(PathBuf, Vec<(String, Vec<String>)>)>,
    file_syntax: Vec<FileSyntaxFacts>,
    incoming_edges: Vec<(String, ErrorEdgeTarget)>,
}

impl CrateTypeIndex {
    #[instrument(level = "debug", skip(self, ir), err(level = "warn"))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn write_to_ir(
        &self,
        ir: &mut dyn IrMut,
    ) -> CordialResult<()> {
        let facts = CrateTypeIndexFacts {
            declarations: self
                .declarations
                .iter()
                .map(|(path, targets)| (path.clone(), targets.clone()))
                .collect(),
            local: self
                .local
                .iter()
                .map(|((path, name), targets)| (path.clone(), name.clone(), targets.clone()))
                .collect(),
            modules: self
                .modules
                .iter()
                .map(|(path, module)| (path.clone(), module.clone()))
                .collect(),
            imports: self
                .imports
                .iter()
                .map(|(path, imports)| (path.clone(), imports.clone()))
                .collect(),
            file_syntax: self.file_syntax.values().cloned().collect(),
            incoming_edges: self.incoming_edges.clone(),
        };
        let root = ir.root()?;
        ir.set_attr(root, TYPE_INDEX_ATTR, serde_json::to_value(facts)?)
    }

    #[instrument(level = "debug", skip(ir), err(level = "warn"))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn from_ir(
        ir: &dyn IrView,
    ) -> CordialResult<Self> {
        let root = ir.root()?;
        let value = ir
            .node(root)
            .and_then(|node| node.attr(TYPE_INDEX_ATTR).cloned())
            .ok_or_else(|| CordialError::unreachable(TYPE_INDEX_ATTR))?;
        let facts: CrateTypeIndexFacts = serde_json::from_value(value)?;
        Ok(Self {
            declarations: facts.declarations.into_iter().collect(),
            local: facts
                .local
                .into_iter()
                .map(|(path, name, targets)| ((path, name), targets))
                .collect(),
            modules: facts.modules.into_iter().collect(),
            imports: facts.imports.into_iter().collect(),
            file_syntax: facts
                .file_syntax
                .into_iter()
                .map(|facts| (facts.path().clone(), facts))
                .collect(),
            incoming_edges: facts.incoming_edges,
        })
    }
}

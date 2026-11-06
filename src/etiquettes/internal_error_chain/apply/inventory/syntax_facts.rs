//! Located identifiers and paths gathered once from a shared source tree.
//! Graph closure uses these facts through the crate index instead of visiting
//! each file again for every candidate graph.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::loader::SourceSnapshot;

use super::super::graph_apply::{GraphDeclarationFacts, GraphExprFacts, GraphLifetimeFacts};
use super::super::initializers::InitializerFileFacts;
use super::super::lifetime::LifetimeFileFacts;
use super::super::names::NameAudit;

pub(super) type LocatedName = (String, usize, usize);

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct FileSyntaxFacts {
    path: std::path::PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    source_identifiers: BTreeSet<String>,
    static_strings: BTreeSet<String>,
    identifiers: BTreeSet<LocatedName>,
    declarations: BTreeSet<LocatedName>,
    paths: Vec<(LocatedName, Vec<String>)>,
    macro_names: BTreeSet<String>,
    #[getter(copy)]
    has_cfg: bool,
    name_audit: NameAudit,
    expressions: GraphExprFacts,
    lifetimes: GraphLifetimeFacts,
    local_lifetimes: LifetimeFileFacts,
    initializers: InitializerFileFacts,
    conversion_sites: super::sites::ConversionFileFacts,
    graph_declarations: GraphDeclarationFacts,
}

impl FileSyntaxFacts {
    #[instrument(
        level = "debug",
        skip(file, tracked_names, tracked_fields, graph_declarations, source),
        err(level = "warn")
    )]
    pub(super) fn collect(
        file: &impl SourceSnapshot,
        tracked_names: &BTreeSet<String>,
        tracked_fields: &BTreeSet<String>,
        graph_declarations: GraphDeclarationFacts,
        source: &crate::ir::SourceErrorApply,
    ) -> crate::CordialResult<Self> {
        let uses = source.uses_for(file)?;
        Ok(Self {
            path: file.path().to_path_buf(),
            source_digest: file.source_digest(),
            source_identifiers: uses
                .source_identifiers()
                .intersection(tracked_names)
                .cloned()
                .collect(),
            static_strings: source.static_strings_for(file)?.clone(),
            identifiers: uses
                .identifiers()
                .iter()
                .filter(|(name, _, _)| tracked_names.contains(name))
                .cloned()
                .collect(),
            declarations: uses
                .declarations()
                .iter()
                .filter(|(name, _, _)| tracked_names.contains(name))
                .cloned()
                .collect(),
            paths: uses
                .paths()
                .iter()
                .filter(|((name, _, _), _)| tracked_names.contains(name))
                .cloned()
                .collect(),
            macro_names: uses
                .macro_names()
                .intersection(tracked_names)
                .cloned()
                .collect(),
            has_cfg: uses.has_cfg(),
            name_audit: source.name_audit_for(file)?.clone(),
            expressions: source
                .expressions_for(file)?
                .select(tracked_names, tracked_fields),
            lifetimes: source.lifetimes_for(file)?.select_names(tracked_names),
            local_lifetimes: source
                .local_lifetimes_for(file)?
                .select_names(tracked_names),
            initializers: source.initializers_for(file)?.clone(),
            conversion_sites: source.conversion_sites_for(file)?.clone(),
            graph_declarations,
        })
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn mentions(&self, name: &str) -> bool {
        self.identifiers.iter().any(|(ident, _, _)| ident == name)
            || self.macro_names.contains(name)
    }

    #[instrument(level = "trace", skip(self, name))]
    pub(super) fn source_mentions(&self, name: &str) -> bool {
        self.source_identifiers.contains(name)
    }
}

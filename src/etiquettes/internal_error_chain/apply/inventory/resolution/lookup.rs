//! Resolve collected declarations and verify graph-wide name evidence.

use std::collections::{BTreeMap, BTreeSet};

use tracing::instrument;

use crate::loader::SourceSnapshot;

use super::super::super::graph_apply::{GraphDeclarationFacts, GraphExprFacts, GraphLifetimeFacts};
use super::super::edges::ErrorEdgeTarget;
use super::super::syntax_facts::FileSyntaxFacts;
use super::CrateTypeIndex;
use super::paths::absolute_path;

impl CrateTypeIndex {
    #[instrument(level = "trace", skip(self, file))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn graph_declarations(
        &self,
        file: &impl SourceSnapshot,
    ) -> Option<&GraphDeclarationFacts> {
        self.syntax_for(file)
            .map(FileSyntaxFacts::graph_declarations)
    }
    #[instrument(level = "trace", skip(self, file))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn expression_facts(
        &self,
        file: &impl SourceSnapshot,
    ) -> Option<&GraphExprFacts> {
        self.syntax_for(file).map(FileSyntaxFacts::expressions)
    }
    #[instrument(level = "trace", skip(self, file))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn lifetime_facts(
        &self,
        file: &impl SourceSnapshot,
    ) -> Option<&GraphLifetimeFacts> {
        self.syntax_for(file).map(FileSyntaxFacts::lifetimes)
    }
    #[instrument(level = "trace", skip(self, file, name))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn file_mentions(
        &self,
        file: &impl SourceSnapshot,
        name: &str,
    ) -> bool {
        self.syntax_for(file)
            .is_some_and(|facts| facts.mentions(name))
    }

    #[instrument(level = "trace", skip(self, file, name))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn source_mentions(
        &self,
        file: &impl SourceSnapshot,
        name: &str,
    ) -> bool {
        self.syntax_for(file)
            .is_none_or(|facts| facts.source_mentions(name))
    }

    #[instrument(level = "trace", skip(self, file, name))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn name_blocks(
        &self,
        file: &impl SourceSnapshot,
        name: &str,
    ) -> bool {
        self.syntax_for(file)
            .is_none_or(|facts| facts.name_audit().blocks(name))
    }

    #[instrument(level = "debug", skip(self, file, expected))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn paths_resolve_to_graph(
        &self,
        file: &impl SourceSnapshot,
        expected: &BTreeMap<String, ErrorEdgeTarget>,
    ) -> bool {
        let Some(facts) = self.syntax_for(file) else {
            return false;
        };
        if facts.has_cfg()
            || facts
                .macro_names()
                .iter()
                .any(|name| expected.contains_key(name))
        {
            return false;
        }
        let allowed_paths: BTreeSet<_> = facts
            .paths()
            .iter()
            .filter_map(|(location, path)| {
                expected
                    .get(&location.0)
                    .filter(|target| self.resolve(file, path).as_ref() == Some(*target))
                    .map(|_| location)
            })
            .collect();
        facts.identifiers().iter().all(|location| {
            let Some(target) = expected.get(&location.0) else {
                return true;
            };
            allowed_paths.contains(location)
                || (facts.declarations().contains(location)
                    && target.declaration() == file.path()
                    && target.line() == location.1
                    && target.column() == location.2)
        })
    }

    #[instrument(level = "trace", skip(self, expected))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn has_incoming_edge_outside_graph(
        &self,
        expected: &BTreeMap<String, ErrorEdgeTarget>,
    ) -> bool {
        self.incoming_edges.iter().any(|(owner, target)| {
            !expected.contains_key(owner) && expected.values().any(|member| member == target)
        })
    }

    #[instrument(level = "trace", skip(self, file))]
    fn syntax_for(&self, file: &impl SourceSnapshot) -> Option<&FileSyntaxFacts> {
        self.file_syntax
            .get(file.path())
            .filter(|facts| facts.source_digest() == file.source_digest())
    }

    #[instrument(level = "trace", skip(self, file, written_path))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn resolve(
        &self,
        file: &impl SourceSnapshot,
        written_path: &[String],
    ) -> Option<ErrorEdgeTarget> {
        if let [name] = written_path
            && let Some(candidates) = self.local.get(&(file.path().to_path_buf(), name.clone()))
        {
            return match candidates.as_slice() {
                [Some(target)] => Some(target.clone()),
                _ => None,
            };
        }
        let module = self.active_modules.get(file.path())?;
        let absolute = absolute_path(&module, written_path)?;
        if let Some(target) = self.unique(&absolute) {
            return Some(target);
        }
        if written_path.len() != 1 {
            return None;
        }
        let name = &written_path[0];
        let matches: Vec<_> = self
            .imports
            .get(file.path())?
            .iter()
            .filter(|(alias, _)| alias == name)
            .collect();
        let [(_, path)] = matches.as_slice() else {
            return None;
        };
        if !path
            .first()
            .is_some_and(|part| matches!(part.as_str(), "crate" | "self" | "super"))
        {
            return None;
        }
        self.unique(&absolute_path(&module, path)?)
    }

    #[instrument(level = "trace", skip(self, path))]
    fn unique(&self, path: &[String]) -> Option<ErrorEdgeTarget> {
        let [Some(target)] = self.declarations.get(path)?.as_slice() else {
            return None;
        };
        Some(target.clone())
    }
}

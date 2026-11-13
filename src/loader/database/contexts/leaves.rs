//! The leaf pass: integration-test binaries read off the library.
//!
//! A test binary consumes the library and nothing consumes it. Its `#[test]`
//! functions are active only under the `test` cfg, so the pass enters that
//! context once, reads each leaf function's calls, and restores the ordinary
//! build. Nothing here adds a node to the library's graph.

use super::LocalCfgs;
use super::plan::{Covered, asked};
use crate::loader::database::{
    SourceDatabase, populate::expanded_definition, queries::FunctionKey, queries::function_evidence,
};
use crate::{CordialResult, CrateIr, FunctionLocation, LeafFunction, SemanticContextKey};
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_syntax::{AstNode, ast, ast::HasName};
use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

/// Most contexts the leaf files may ask for before the rest are reported as a gap.
const LEAF_CONTEXT_LIMIT: usize = 64;
use tracing::instrument;

impl SourceDatabase {
    /// Read the crate's observability inventory again in the cfg context the
    /// database is in and add it to the one the ordinary build produced, so a
    /// print or event in code only this context activates is seen.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn merge_context_observability(
        &self,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        use crate::IrView;
        let mut inventory = graph.source_observability()?;
        let seen = self.project_observability(inventory.crate_root())?;
        inventory.merge_context(seen);
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            serde_json::to_value(inventory)?,
        )
    }

    /// Record, under `ATTR_LEAF_FUNCTIONS`, what each function of the crate's
    /// integration-test files calls.
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn project_leaves(
        &mut self,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        use crate::IrView;
        let files: Vec<PathBuf> = graph
            .source_observability()?
            .files()
            .iter()
            .filter(|file| file.kind() == crate::ir::ObservabilityFileKind::Test)
            .map(|file| file.snapshot().path().clone())
            .collect();
        let mut leaves: Vec<LeafFunction> = Vec::new();
        if !files.is_empty() {
            self.read_leaf_contexts(&files, &mut leaves)?;
        }
        graph.set_attr(
            graph.root(),
            crate::ATTR_LEAF_FUNCTIONS,
            serde_json::to_value(leaves)?,
        )
    }

    /// Read the leaf files under the `test` cfg and under every context their own
    /// gates ask for. A leaf function gated by a feature or another cfg is active
    /// only where that gate holds, and the gates are taken one clause at a time:
    /// merging them, as the library's planning does, can set one atom both ways
    /// for a file that needs it on. Repeats until no gate asks for more.
    #[instrument(level = "debug", skip(self, files, leaves), err(level = "warn"))]
    fn read_leaf_contexts(
        &mut self,
        files: &[PathBuf],
        leaves: &mut Vec<LeafFunction>,
    ) -> CordialResult<()> {
        let mut only = BTreeSet::new();
        for path in files {
            only.insert(self.file_id(path)?);
        }
        let cfgs = LocalCfgs::capture(&self.db);
        let base = SemanticContextKey::enabling(["test"]);
        let mut covered = Covered::default();
        let mut seen = vec![base.clone()];
        let mut queue = VecDeque::from([base]);
        let mut outcome = Ok(());
        while let Some(context) = queue.pop_front() {
            if seen.len() > LEAF_CONTEXT_LIMIT {
                tracing::warn!(
                    target: "cordial::semantic::leaves",
                    queued = queue.len() + 1,
                    "leaf context planning stopped before its fixed point"
                );
                break;
            }
            cfgs.enter(&mut self.db, &context);
            let mut found = Vec::new();
            outcome = self.read_leaves(files, &mut found);
            tracing::debug!(target: "cordial::semantic::leaves", %context, found = found.len(), "leaf context read");
            merge(leaves, found);
            if outcome.is_err() {
                break;
            }
            for next in asked(&self.db, &context, &mut covered, Some(&only)) {
                if !seen.contains(&next) {
                    seen.push(next.clone());
                    queue.push_back(next);
                }
            }
        }
        cfgs.restore(&mut self.db);
        outcome
    }

    #[instrument(level = "debug", skip(self, files, leaves), err(level = "warn"))]
    fn read_leaves(&self, files: &[PathBuf], leaves: &mut Vec<LeafFunction>) -> CordialResult<()> {
        attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            let roots = self
                .projection_context()?
                .packages()
                .iter()
                .map(|package| package.root().clone())
                .collect();
            let locator = super::super::external::Locator::new(&self.db, &self.vfs, roots);
            for path in files {
                let file = self.file_id(path)?;
                let tree = sema.parse(sema.attach_first_edition(file));
                for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
                    let Some(definition) = sema
                        .to_def(&function)
                        .or_else(|| expanded_definition(&sema, &function))
                    else {
                        tracing::debug!(
                            target: "cordial::semantic::leaves",
                            file = %path.display(),
                            function = function.name().map(|name| name.to_string()).unwrap_or_default(),
                            "leaf function with no definition under the test cfg"
                        );
                        continue;
                    };
                    let Some(facts) =
                        function_evidence(&self.db, FunctionKey::new(&self.db, definition))
                            .as_ref()
                    else {
                        continue;
                    };
                    let callees = facts
                        .calls()
                        .iter()
                        .filter_map(|call| call.callee().clone())
                        .collect();
                    let called = facts
                        .calls()
                        .iter()
                        .filter_map(|call| match call.target() {
                            crate::SemanticCallTarget::Function(id) => locator.locate(*id),
                            _ => None,
                        })
                        .collect();
                    let start = u32::from(function.syntax().text_range().start());
                    leaves.push(LeafFunction::new(
                        FunctionLocation::new(path.clone(), start),
                        callees,
                        called,
                    ));
                }
            }
            Ok(())
        })
    }
}

/// Add `found` to `leaves`, joining a function seen in more than one context.
#[instrument(level = "trace", skip_all)]
fn merge(leaves: &mut Vec<LeafFunction>, found: Vec<LeafFunction>) {
    for leaf in found {
        match leaves
            .iter_mut()
            .find(|known| known.location() == leaf.location())
        {
            Some(known) => {
                let mut callees = known.callees().clone();
                let mut called = known.called().clone();
                for callee in leaf.callees() {
                    if !callees.contains(callee) {
                        callees.push(callee.clone());
                    }
                }
                for location in leaf.called() {
                    if !called.contains(location) {
                        called.push(location.clone());
                    }
                }
                *known = LeafFunction::new(known.location().clone(), callees, called);
            }
            None => leaves.push(leaf),
        }
    }
}

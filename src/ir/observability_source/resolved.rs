//! Facts the database resolved for the crate's functions, joined to the
//! collected inventory, and the merge of inventories read in several cfg contexts.

use super::SourceObservability;
use std::path::Path;

/// What the database resolved for one function: the callees of its calls, and where
/// the workspace functions among them are defined.
#[derive(Debug, Default, derive_getters::Getters, derive_new::new)]
pub(crate) struct ResolvedCalls {
    callees: Vec<crate::ir::SemanticCallee>,
    called: Vec<crate::FunctionLocation>,
    /// Whether the function returns a `Result`, as the solver resolved its type.
    fallible: bool,
}

impl SourceObservability {
    /// Fill each function's call-derived facts from what the database resolved
    /// for it. `resolve` is given the file and the function's range and returns
    /// what the function calls in every context that resolved it; a function with
    /// no facts keeps nothing, and nothing is claimed for it.
    #[tracing::instrument(level = "debug", skip(self, resolve))]
    pub(crate) fn apply_calls(
        &mut self,
        mut resolve: impl FnMut(&Path, std::ops::Range<usize>) -> ResolvedCalls,
    ) {
        for file in &mut self.files {
            let path = file.snapshot.path().clone();
            for function in &mut file.functions {
                let range = function.range.start()..function.range.end();
                let resolved = resolve(&path, range);
                for callee in resolved.callees() {
                    function.body.note_callee(callee);
                }
                function.body.called = resolved.called().clone();
                function.body.is_fallible |= *resolved.fallible();
            }
        }
    }
}

impl SourceObservability {
    /// Add what the same crate showed when read in another cfg context. Files and
    /// functions are matched by place, so the facts of one function from two
    /// contexts join; a print is added unless this context already saw it.
    #[tracing::instrument(level = "debug", skip(self, other))]
    pub(crate) fn merge_context(&mut self, other: Self) {
        for file in other.files {
            let Some(known) = self
                .files
                .iter_mut()
                .find(|known| known.relative == file.relative)
            else {
                self.files.push(file);
                continue;
            };
            for function in file.functions {
                match known
                    .functions
                    .iter_mut()
                    .find(|known| known.range == function.range)
                {
                    Some(known) => known.body.merge(&function.body),
                    None => known.functions.push(function),
                }
            }
            for print in file.prints {
                if !known.prints.contains(&print) {
                    known.prints.push(print);
                }
            }
        }
        self.files.sort_by(|a, b| a.relative.cmp(&b.relative));
    }
}

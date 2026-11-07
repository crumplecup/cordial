//! Plan file-local tracing edits before the caller writes the full transaction.

use std::path::Path;

use crate::PathInclusionFacts;
use crate::config::CordialConfig;
use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::loader::SourceLoadView;
use crate::source_write::SourceWrite;

use super::super::CallGraphFacts;
use super::super::types::TracingRuleKind;
use super::instrument::{
    GapApplyOutcome, InstrumentAttrStyle, apply_gap, attr_style, ensure_use_instrument,
    recipe_for_gap, strip_instrument,
};
use super::ir::{recipes_for_file, snapshot_for_file};
use super::lint_policy::ApplyLintPolicy;
use super::{
    InstrumentApplySummary, InstrumentGap, TracingApplyPolicy, dedupe_gaps,
    resolve_tracing_apply_policy,
};

#[derive(derive_new::new)]
pub(super) struct FilePlanner<'a> {
    project_root: &'a Path,
    source_view: &'a SourceLoadView,
    workspace: &'a WorkspaceIr,
    config: &'a CordialConfig,
    facts: &'a PathInclusionFacts,
    call_graph: &'a CallGraphFacts,
}

impl FilePlanner<'_> {
    #[tracing::instrument(level = "debug", skip(self, file_gaps, summary), err(level = "warn"))]
    pub(super) fn plan_file(
        &self,
        crate_name: &str,
        crate_root: &Path,
        rel_path: &Path,
        mut file_gaps: Vec<InstrumentGap>,
        summary: &mut InstrumentApplySummary,
    ) -> CordialResult<Option<(SourceWrite, Vec<InstrumentGap>)>> {
        let path = crate_root.join(rel_path);
        let controls = ApplyLintPolicy::new(
            self.config,
            crate::LintLocation::new(self.project_root, Some(crate_root), crate_name, Some(&path)),
        );
        if !path.is_file() {
            tracing::warn!(
                path = %path.display(),
                crate_name = %crate_name,
                "checklist path does not exist"
            );
            summary.unresolved += file_gaps.len();
            return Ok(None);
        }

        let snapshot = snapshot_for_file(self.workspace, crate_name, &path)?;
        let edit = self.source_view.edit_file(&snapshot)?;
        if !edit.matches_disk()? {
            return Err(crate::CordialError::stale_source_snapshot(&path));
        }
        let source = edit.source();
        let crlf = source.matches("\r\n").count();
        if crlf > 0 && crlf != source.bytes().filter(|byte| *byte == b'\n').count() {
            tracing::warn!(path = %path.display(), "mixed line endings require a byte-preserving edit path");
            summary.unresolved += file_gaps.len();
            return Ok(None);
        }
        let never_instrument = self.call_graph.never_instrument(crate_name);
        let records = match recipes_for_file(self.workspace, crate_name, rel_path) {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    crate_name = %crate_name,
                    error = %error,
                    "failed to classify functions for apply"
                );
                summary.unresolved += file_gaps.len();
                return Ok(None);
            }
        };
        let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
        dedupe_gaps(&mut file_gaps);
        file_gaps.sort_by_key(|right| std::cmp::Reverse(right.line()));
        let style = attr_style(&lines);
        let policy = resolve_tracing_apply_policy(
            crate_name,
            &path,
            crate_root,
            self.config.tracing(),
            self.facts,
        );
        if policy == TracingApplyPolicy::Skip {
            tracing::info!(
                path = %path.display(),
                crate_name = %crate_name,
                "skip-policy file: apply strips existing #[instrument], never writes one"
            );
        }

        let mut file_changed = false;
        let mut needs_import = false;
        let mut file_applied = Vec::new();
        for gap in file_gaps {
            let record = recipe_for_gap(&records, &gap);
            let strip = never_instrument.contains(gap.qualified_name())
                || policy == TracingApplyPolicy::Skip;
            let strip_kind = if never_instrument.contains(gap.qualified_name()) {
                TracingRuleKind::ProofInstrument
            } else {
                TracingRuleKind::SkipInstrument
            };
            let outcome = if strip && !controls.enabled(strip_kind) {
                GapApplyOutcome::SkippedPolicy
            } else if strip {
                // Proof/skip-only functions may intentionally have no recipe.
                // Without a located annotation there is no authorized edit.
                record.map_or(GapApplyOutcome::SkippedPolicy, |record| {
                    strip_instrument(&mut lines, source, record)
                })
            } else if let Some(record) = record {
                let outcome =
                    apply_gap(&mut lines, source, &gap, record, style, &policy, &controls)?;
                if outcome == GapApplyOutcome::Applied {
                    needs_import |= !record.attributes().iter().any(|attr| attr.instrument())
                        && style == InstrumentAttrStyle::Short
                        && policy == TracingApplyPolicy::Bare;
                }
                outcome
            } else {
                tracing::warn!(
                    path = %gap.rel_path().display(),
                    line = gap.line(),
                    qualified_name = %gap.qualified_name(),
                    "no classified recipe for checklist item"
                );
                GapApplyOutcome::Unresolved
            };
            match outcome {
                GapApplyOutcome::Applied => {
                    summary.changed_functions += 1;
                    file_changed = true;
                    file_applied.push(gap);
                }
                GapApplyOutcome::AlreadyInstrumented => summary.skipped_existing += 1,
                GapApplyOutcome::Unresolved => summary.unresolved += 1,
                GapApplyOutcome::SkippedPolicy => summary.skipped_policy += 1,
            }
        }

        if file_changed {
            // Only `Bare` ever writes the short `instrument(..)` form
            // that needs this import -- `Gated` always renders fully
            // qualified (see `apply_gap`), so inserting it regardless
            // of `policy` would leave a real `unused_imports` warning
            // in any file where every applied gap was `Gated`.
            if needs_import {
                lines = ensure_use_instrument(lines);
            }
            let newline = if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let mut replacement = lines.join(newline);
            if source.ends_with('\n') {
                replacement.push_str(newline);
            }
            // Validate proposed edits before any source in the transaction changes.
            syn::parse_file(&replacement).map_err(|error| {
                crate::CordialError::syn_parse(path.display().to_string(), error)
            })?;
            summary.changed_files += 1;
            return Ok(Some((
                SourceWrite::new(path, source.to_string(), replacement),
                file_applied,
            )));
        }
        Ok(None)
    }
}

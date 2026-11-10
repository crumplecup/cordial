use std::collections::HashSet;
use std::path::Path;

use crate::error::CordialResult;
use crate::ir::SourceTracingInventory;

use super::recipe::recipe as instrument_recipe;
use super::types::FunctionRecord;

use tracing::instrument;

impl SourceTracingInventory {
    /// Apply run-specific configuration and proof reachability to collected facts.
    #[instrument(
        level = "debug",
        skip(self, never_instrument, file),
        err(level = "warn")
    )]
    pub(crate) fn records(
        &self,
        file: &Path,
        module_prefix: &str,
        project_root: &Path,
        extra_skip: &[String],
        never_instrument: &HashSet<String>,
    ) -> CordialResult<Vec<FunctionRecord>> {
        let relative = file
            .strip_prefix(project_root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let mut records = Vec::new();
        for function in self.functions() {
            let qualified = if module_prefix.is_empty() {
                function.path().clone()
            } else {
                format!("{module_prefix}::{}", function.path())
            };
            let proof_only = never_instrument.contains(&qualified);
            if proof_only && !function.instrumented() {
                continue;
            }
            let ctx = function.context();
            records.push(
                FunctionRecord::builder()
                    .qualified_name(qualified)
                    .kind(function.kind())
                    .visibility(function.visibility().clone())
                    .file(relative.clone())
                    .line(function.line())
                    .instrumented(function.instrumented())
                    .proof_only(proof_only)
                    .prover_visible_instrument(function.prover_visible_instrument())
                    .has_error_path_event(ctx.has_error_path_event())
                    .param_names(ctx.param_names().clone())
                    .role(ctx.role())
                    .complexity(ctx.complexity())
                    .recipe(instrument_recipe(ctx, extra_skip)?)
                    .build()?,
            );
        }
        Ok(records)
    }
}

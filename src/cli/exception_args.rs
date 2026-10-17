//! Clap arguments and conversion for exception edits.

use tracing::instrument;

use super::commands::ActCtx;
use crate::{ExceptionSelector, ExceptionUpdate};

/// Selects one quality exception row: etiquette, crate, and row fields.
#[derive(clap::Args)]
pub struct ExceptionTargetArgs {
    /// Quality etiquette id (for example `panics`).
    pub(super) etiquette: String,
    /// Crate name (default: `--crate-name` or the project directory).
    #[arg(long)]
    crate_name: Option<String>,
    /// Select the row with this rule id.
    #[arg(long)]
    rule_id: Option<String>,
    /// Select the row with this context / qualified name.
    #[arg(long)]
    context: Option<String>,
    /// Select the row with this source file.
    #[arg(long)]
    file: Option<String>,
    /// Select the row with this line.
    #[arg(long)]
    line: Option<u32>,
}

/// Replacement values for `exceptions edit`; unset fields keep their value.
#[derive(clap::Args)]
pub struct ExceptionNewArgs {
    /// Replacement source file.
    #[arg(long)]
    new_file: Option<String>,
    /// Replacement line.
    #[arg(long)]
    new_line: Option<u32>,
    /// Replacement rule id (empty clears it).
    #[arg(long)]
    new_rule_id: Option<String>,
    /// Replacement context / qualified name (empty clears it).
    #[arg(long)]
    new_context: Option<String>,
    /// Replacement reason.
    #[arg(long)]
    new_reason: Option<String>,
}

impl ExceptionTargetArgs {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn selector(&self) -> ExceptionSelector {
        let mut selector = ExceptionSelector::default();
        if let Some(file) = &self.file {
            selector = selector.with_file(file.clone());
        }
        if let Some(line) = self.line {
            selector = selector.with_line(line);
        }
        if let Some(rule_id) = &self.rule_id {
            selector = selector.with_rule_id(rule_id.clone());
        }
        if let Some(context) = &self.context {
            selector = selector.with_context(context.clone());
        }
        selector
    }

    #[instrument(level = "trace", skip(self, ctx))]
    pub(super) fn crate_name(&self, ctx: &ActCtx) -> String {
        self.crate_name
            .clone()
            .or_else(|| ctx.crate_name().clone())
            .unwrap_or_else(|| ctx.store().project_slug().clone())
    }
}

impl ExceptionNewArgs {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn update(self) -> ExceptionUpdate {
        let mut update = ExceptionUpdate::default();
        if let Some(file) = self.new_file {
            update = update.with_file(file);
        }
        if let Some(line) = self.new_line {
            update = update.with_line(line);
        }
        if let Some(rule_id) = self.new_rule_id {
            update = update.with_rule_id(rule_id);
        }
        if let Some(context) = self.new_context {
            update = update.with_context(context);
        }
        if let Some(reason) = self.new_reason {
            update = update.with_reason(reason);
        }
        update
    }
}

//! Compare a classified recipe to the attribute (and events) already present.

use super::present::PresentInstrument;
use super::types::{FunctionRole, InstrumentRecipe, TracingRuleKind};

use tracing::instrument;
/// Inputs the delta rules read besides the recipe itself.
#[derive(Debug, Clone)]
pub struct DeltaContext<'a> {
    pub role: FunctionRole,
    pub param_names: &'a [String],
    pub has_error_path_event: bool,
}

/// Recipe-vs-present findings for an already-instrumented function.
#[instrument(level = "debug", skip(recipe, present, ctx))]
pub fn recipe_deltas(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    ctx: &DeltaContext<'_>,
) -> Vec<TracingRuleKind> {
    let mut kinds = Vec::new();
    if present.level() > recipe.level() {
        kinds.push(TracingRuleKind::LevelMismatch);
    }
    if skip_missing(recipe, present, ctx.param_names) {
        kinds.push(TracingRuleKind::SkipMissing);
    }
    if recipe.err().is_some() && !present.err() {
        kinds.push(TracingRuleKind::ErrMissing);
    }
    if error_path_silent(recipe, present, ctx) {
        kinds.push(TracingRuleKind::ErrorPathSilent);
    }
    if matches!(ctx.role, FunctionRole::Entry | FunctionRole::Constructor)
        && fields_missing(recipe, present)
    {
        kinds.push(TracingRuleKind::FieldsMissing);
    }
    kinds
}

#[instrument(level = "debug", skip(recipe, present))]
fn skip_missing(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    param_names: &[String],
) -> bool {
    if present.skip_all() {
        return false;
    }
    recipe.skip().iter().any(|name| {
        param_names.iter().any(|param| param == name)
            && !present.skip().iter().any(|got| got == name)
    })
}

#[instrument(level = "debug", skip(recipe, present))]
fn fields_missing(recipe: &InstrumentRecipe, present: &PresentInstrument) -> bool {
    recipe
        .fields()
        .iter()
        .any(|name| !present.fields().iter().any(|got| got == name))
}

#[instrument(level = "debug", skip(recipe, present, ctx))]
fn error_path_silent(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    ctx: &DeltaContext<'_>,
) -> bool {
    if present.err() || ctx.has_error_path_event {
        return false;
    }
    recipe.err().is_some()
}

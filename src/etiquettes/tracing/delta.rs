//! Compare a classified recipe to the attribute (and events) already present.

use super::present::PresentInstrument;
use super::types::{FunctionComplexity, FunctionRole, InstrumentRecipe, TracingRuleKind};

/// Inputs the delta rules read besides the recipe itself.
#[derive(Debug, Clone)]
pub struct DeltaContext<'a> {
    pub role: FunctionRole,
    #[allow(dead_code)] // kept for later strategy reads
    pub complexity: FunctionComplexity,
    #[allow(dead_code)] // was used for error-site join; recipe.err is the silent-path gate
    pub qualified_path: &'a str,
    pub param_names: &'a [String],
    pub has_error_path_event: bool,
}

/// Recipe-vs-present findings for an already-instrumented function.
#[tracing::instrument(level = "debug", skip(recipe, present, ctx))]
pub fn recipe_deltas(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    ctx: &DeltaContext<'_>,
) -> Vec<TracingRuleKind> {
    let mut kinds = Vec::new();
    if present.level > recipe.level {
        kinds.push(TracingRuleKind::LevelMismatch);
    }
    if skip_missing(recipe, present, ctx.param_names) {
        kinds.push(TracingRuleKind::SkipMissing);
    }
    if recipe.err.is_some() && !present.err {
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

fn skip_missing(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    param_names: &[String],
) -> bool {
    if present.skip_all {
        return false;
    }
    recipe.skip.iter().any(|name| {
        param_names.iter().any(|param| param == name) && !present.skip.iter().any(|got| got == name)
    })
}

fn fields_missing(recipe: &InstrumentRecipe, present: &PresentInstrument) -> bool {
    recipe
        .fields
        .iter()
        .any(|name| !present.fields.iter().any(|got| got == name))
}

fn error_path_silent(
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    ctx: &DeltaContext<'_>,
) -> bool {
    if present.err || ctx.has_error_path_event {
        return false;
    }
    recipe.err.is_some()
}

//! Compare a classified recipe to the attribute (and events) already present.

use crate::ir::{IrView, NodeKind, Query};

use super::present::PresentInstrument;
use super::types::{FunctionComplexity, FunctionRole, InstrumentRecipe, TracingRuleKind};

/// Inputs the delta rules read besides the recipe itself.
#[derive(Debug, Clone)]
pub struct DeltaContext<'a> {
    pub role: FunctionRole,
    pub complexity: FunctionComplexity,
    pub qualified_path: &'a str,
    pub param_names: &'a [String],
    pub has_error_path_event: bool,
}

/// Recipe-vs-present findings for an already-instrumented function.
#[tracing::instrument(skip(ir, recipe, present, ctx), fields(path = ctx.qualified_path, role = %ctx.role))]
pub fn recipe_deltas(
    ir: &dyn IrView,
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
    if error_path_silent(ir, recipe, present, ctx) {
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
    ir: &dyn IrView,
    recipe: &InstrumentRecipe,
    present: &PresentInstrument,
    ctx: &DeltaContext<'_>,
) -> bool {
    if present.err || ctx.has_error_path_event {
        return false;
    }
    recipe.err.is_some()
        || ctx.complexity == FunctionComplexity::Fallible
        || fn_has_error_site(ir, ctx.qualified_path)
}

/// True when error-sites (if loaded) attached a site under this function path.
fn fn_has_error_site(ir: &dyn IrView, qualified_path: &str) -> bool {
    for node in ir.nodes_matching(&ERROR_SITE_QUERY) {
        let Some(context) = node.attr("context").and_then(|value| value.as_str()) else {
            continue;
        };
        if context_matches_fn(context, qualified_path) {
            return true;
        }
    }
    false
}

fn context_matches_fn(context: &str, qualified_path: &str) -> bool {
    context == qualified_path
        || context.ends_with(&format!("::{qualified_path}"))
        || qualified_path.ends_with(&format!("::{context}"))
}

struct ErrorSiteQuery;

impl Query for ErrorSiteQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[crate::ir::EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn crate::ir::NodeView) -> bool {
        node.attr("error_site_kind").is_some()
    }
}

static ERROR_SITE_QUERY: ErrorSiteQuery = ErrorSiteQuery;

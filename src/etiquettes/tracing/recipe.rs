//! Per-variant instrument recipes. Dispatch is a `match` on [`FunctionRole`].

use super::types::{
    FnContext, FunctionComplexity, FunctionRole, InstrumentLevel, InstrumentRecipe,
};

use tracing::instrument;
/// Parameter names skipped by default (bulky or unhelpful in a span).
pub(super) const DEFAULT_SKIP_PARAMS: &[&str] = &[
    "binary_entry_files",
    "build",
    "cache",
    "conn",
    "connection",
    "ctx",
    "data",
    "err",
    "error",
    "file",
    "findings",
    "formatter",
    "inventory",
    "items",
    "manifest",
    "msg",
    "options",
    "other",
    "path",
    "report",
    "self",
    "snapshots",
    "source",
    "syntax",
    "ui",
    "workspace",
];

const IDENTITY_PARAMS: &[&str] = &["crate_name", "crate", "name", "id"];

/// Target recipe for a classified function.
#[instrument(level = "debug", skip(ctx))]
pub fn recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    match ctx.role {
        FunctionRole::Constructor => constructor_recipe(ctx, extra_skip),
        FunctionRole::Getter => getter_recipe(ctx, extra_skip),
        FunctionRole::Setter => setter_recipe(ctx, extra_skip),
        FunctionRole::Predicate => predicate_recipe(ctx, extra_skip),
        FunctionRole::Scan => scan_recipe(ctx, extra_skip),
        FunctionRole::Io => io_recipe(ctx, extra_skip),
        FunctionRole::Render => render_recipe(ctx, extra_skip),
        FunctionRole::TraitSurface => trait_surface_recipe(ctx, extra_skip),
        FunctionRole::Entry => entry_recipe(ctx, extra_skip),
        FunctionRole::Other => other_recipe(ctx, extra_skip),
    }
}

#[instrument(level = "debug", skip(ctx))]
fn constructor_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Debug,
        skip: skip_params(ctx, extra_skip),
        fields: identity_fields(ctx, extra_skip),
        err: fallible_err(ctx),
        ret: !ctx.return_unrecordable,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn getter_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Trace,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn setter_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Trace,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn predicate_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Trace,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: None,
        ret: ctx.complexity == FunctionComplexity::Trivial && !ctx.return_unrecordable,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn scan_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Debug,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn io_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Info,
        skip: skip_params(ctx, extra_skip),
        fields: identity_fields(ctx, extra_skip),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn render_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Debug,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn trait_surface_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Trace,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: None,
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn entry_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    InstrumentRecipe {
        level: InstrumentLevel::Info,
        skip: skip_params(ctx, extra_skip),
        fields: identity_fields(ctx, extra_skip),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn other_recipe(ctx: &FnContext, extra_skip: &[String]) -> InstrumentRecipe {
    let level = if ctx.complexity == FunctionComplexity::Hotspot {
        InstrumentLevel::Info
    } else {
        InstrumentLevel::Debug
    };
    InstrumentRecipe {
        level,
        skip: skip_params(ctx, extra_skip),
        fields: Vec::new(),
        err: fallible_err(ctx),
        ret: false,
    }
}

#[instrument(level = "debug", skip(ctx))]
fn skip_params(ctx: &FnContext, extra_skip: &[String]) -> Vec<String> {
    ctx.param_names
        .iter()
        .filter(|name| is_skip_param(name, extra_skip, &ctx.unrecordable_params))
        .cloned()
        .collect()
}

#[instrument(level = "debug", skip(ctx))]
fn identity_fields(ctx: &FnContext, extra_skip: &[String]) -> Vec<String> {
    ctx.param_names
        .iter()
        .filter(|name| {
            IDENTITY_PARAMS.contains(&name.as_str())
                && !is_skip_param(name, extra_skip, &ctx.unrecordable_params)
        })
        .cloned()
        .collect()
}

#[instrument(level = "trace", ret)]
fn is_skip_param(name: &str, extra_skip: &[String], unrecordable: &[String]) -> bool {
    DEFAULT_SKIP_PARAMS.contains(&name)
        || extra_skip.iter().any(|skip| skip == name)
        || unrecordable.iter().any(|skip| skip == name)
}

#[instrument(level = "debug", skip(ctx))]
fn fallible_err(ctx: &FnContext) -> Option<InstrumentLevel> {
    if ctx.returns_result && !ctx.return_borrowed && ctx.err_is_displayable {
        Some(InstrumentLevel::Warn)
    } else {
        None
    }
}

//! Tracing probe/assessor finding types.

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use super::class::{
    FunctionComplexity, FunctionKind, FunctionRole, InstrumentRecipe, VisibilityLabel,
};
use tracing::instrument;

/// One discovered function before IR materialization.
#[derive(Debug, Clone)]
pub struct FunctionRecord {
    pub crate_name: String,
    pub qualified_name: String,
    pub kind: FunctionKind,
    pub visibility: VisibilityLabel,
    pub file: String,
    pub line: u32,
    pub instrumented: bool,
    pub has_error_path_event: bool,
    pub param_names: Vec<String>,
    pub role: FunctionRole,
    pub complexity: FunctionComplexity,
    pub recipe: InstrumentRecipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingRuleKind {
    MissingInstrument,
    LevelMismatch,
    SkipMissing,
    ErrMissing,
    ErrorPathSilent,
    FieldsMissing,
}

impl TracingRuleKind {
    #[instrument(level = "debug", skip(self))]
    pub fn rule_id(self) -> &'static str {
        match self {
            Self::MissingInstrument => "TRACING-MISSING-INSTRUMENT",
            Self::LevelMismatch => "TRACING-LEVEL-MISMATCH",
            Self::SkipMissing => "TRACING-SKIP-MISSING",
            Self::ErrMissing => "TRACING-ERR-MISSING",
            Self::ErrorPathSilent => "TRACING-ERROR-PATH-SILENT",
            Self::FieldsMissing => "TRACING-FIELDS-MISSING",
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub fn description(self) -> &'static str {
        match self {
            Self::MissingInstrument => "Function missing `#[instrument]` (recipe on the finding)",
            Self::LevelMismatch => {
                "Recorded `#[instrument]` level is coarser than the recipe (default info)"
            }
            Self::SkipMissing => "Recipe `skip` names are live params and absent from `skip(...)`",
            Self::ErrMissing => {
                "Recipe wants `err` and the attribute has neither `err` nor `err(level = ...)`"
            }
            Self::ErrorPathSilent => {
                "Recipe wants `err` and the body has neither `err` nor `warn!`/`error!`"
            }
            Self::FieldsMissing => "Recipe identity `fields` are missing from `fields(...)`",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TracingRule {
    pub kind: TracingRuleKind,
}

impl TracingRule {
    #[instrument(level = "debug", skip(kind), ret)]
    pub fn new(kind: TracingRuleKind) -> Self {
        Self { kind }
    }
}

impl Rule for TracingRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "tracing"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        self.kind.description()
    }
}

pub const MISSING_INSTRUMENT_LABEL: &str = "missing-instrument";
pub const RECIPE_DELTA_LABEL: &str = "recipe-delta";

#[derive(Debug, Clone)]
pub struct TracingMarker {
    pub anchor: crate::objects::NodeAnchor,
    pub label: &'static str,
}

impl Marker for TracingMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        self.label
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        self.label
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self))]
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct TracingFinding {
    pub rule: TracingRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub qualified_name: String,
    pub kind: FunctionKind,
    pub role: FunctionRole,
    pub complexity: FunctionComplexity,
    pub recipe: InstrumentRecipe,
    pub visibility: VisibilityLabel,
    pub span: FileSpan,
}

impl Finding for TracingFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        self.disposition
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("kind", &self.rule.id());
        sink.field("rule", &self.rule.id());
        sink.field("context", &self.qualified_name);
        sink.field("qualified_name", &self.qualified_name);
        sink.field("function_kind", &self.kind);
        sink.field("role", &self.role);
        sink.field("complexity", &self.complexity);
        sink.field("recipe", &self.recipe.as_attribute());
        sink.field("level", &self.recipe.level);
        sink.field("skip", &self.recipe.skip.join(","));
        sink.field(
            "err",
            &self
                .recipe
                .err
                .map(|level| level.to_string())
                .unwrap_or_default(),
        );
        sink.field("ret", &self.recipe.ret);
        sink.field("visibility", &self.visibility);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
    }
}

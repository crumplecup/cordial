//! Tracing probe/assessor finding types.

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use super::class::{
    FunctionComplexity, FunctionKind, FunctionRole, InstrumentRecipe, VisibilityLabel,
};
use tracing::instrument;

/// One discovered function before IR materialization.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct FunctionRecord {
    crate_name: String,
    qualified_name: String,
    #[getter(copy)]
    kind: FunctionKind,
    visibility: VisibilityLabel,
    file: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    instrumented: bool,
    /// Function is reachable only from proof-only entry points. Uninstrumented
    /// proof-only functions are not recorded; instrumented ones are, so
    /// attenuation can tell the user to remove the span.
    #[getter(copy)]
    proof_only: bool,
    /// At least one `#[instrument]` is *not* wrapped in
    /// `#[cfg_attr(not(<gate>), …)]` — a prover that sets that cfg will
    /// still expand it.
    #[getter(copy)]
    prover_visible_instrument: bool,
    #[getter(copy)]
    has_error_path_event: bool,
    param_names: Vec<String>,
    #[getter(copy)]
    role: FunctionRole,
    #[getter(copy)]
    complexity: FunctionComplexity,
    recipe: InstrumentRecipe,
}

impl FunctionRecord {
    /// Start a builder for this value.
    pub fn builder() -> FunctionRecordBuilder {
        FunctionRecordBuilder::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingRuleKind {
    MissingInstrument,
    LevelMismatch,
    SkipMissing,
    ErrMissing,
    ErrorPathSilent,
    FieldsMissing,
    /// Proof-only function already has `#[instrument]` (bare or gated).
    ProofInstrument,
    /// Ordinary function in a gate-policy file has bare `#[instrument]`.
    UngatedInstrument,
    /// Skip-policy file (Verus / Creusot) already has `#[instrument]`.
    SkipInstrument,
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
            Self::ProofInstrument => "TRACING-PROOF-INSTRUMENT",
            Self::UngatedInstrument => "TRACING-UNGATED-INSTRUMENT",
            Self::SkipInstrument => "TRACING-SKIP-INSTRUMENT",
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
            Self::ProofInstrument => {
                "Proof-only function has `#[instrument]` (including a `not(<gate>)` wrap that never fires)"
            }
            Self::UngatedInstrument => {
                "Bare `#[instrument]` on a function a verifier build will compile; wrap with `cfg_attr(not(<gate>), …)`"
            }
            Self::SkipInstrument => {
                "Skip-policy file (Verus / Creusot) has `#[instrument]`; remove it"
            }
        }
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct TracingRule {
    pub(super) kind: TracingRuleKind,
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
pub const FORBIDDEN_INSTRUMENT_LABEL: &str = "forbidden-instrument";

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct TracingMarker {
    anchor: crate::objects::NodeAnchor,
    label: String,
}

impl Marker for TracingMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        &self.label
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        &self.label
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

#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct TracingFinding {
    rule: TracingRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    qualified_name: String,
    #[getter(copy)]
    kind: FunctionKind,
    #[getter(copy)]
    role: FunctionRole,
    #[getter(copy)]
    complexity: FunctionComplexity,
    recipe: InstrumentRecipe,
    visibility: VisibilityLabel,
    span: FileSpan,
}

impl TracingFinding {
    /// Start a builder for this value.
    pub fn builder() -> TracingFindingBuilder {
        TracingFindingBuilder::default()
    }
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
        sink.field("recipe", &self.recipe_field());
        sink.field("level", &self.recipe.level());
        sink.field("skip", &self.recipe.skip().join(","));
        sink.field(
            "err",
            &self
                .recipe
                .err()
                .map(|level| level.to_string())
                .unwrap_or_default(),
        );
        sink.field("ret", &self.recipe.ret());
        sink.field("visibility", &self.visibility);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
    }
}

impl TracingFinding {
    #[instrument(level = "trace", skip(self))]
    fn recipe_field(&self) -> String {
        match self.rule.kind {
            TracingRuleKind::ProofInstrument | TracingRuleKind::SkipInstrument => {
                "remove #[instrument]".to_string()
            }
            _ => self.recipe.as_attribute(),
        }
    }
}

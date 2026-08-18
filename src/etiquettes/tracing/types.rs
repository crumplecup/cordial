use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// How a discovered function is categorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Free,
    InherentMethod,
    TraitImplMethod,
}

/// Use-class for an instrument recipe. Dispatch is a `match` on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FunctionRole {
    Constructor,
    Getter,
    Setter,
    Predicate,
    Scan,
    Io,
    Render,
    TraitSurface,
    Entry,
    Other,
}

impl FunctionRole {
    pub const ALL: [Self; 10] = [
        Self::Constructor,
        Self::Getter,
        Self::Setter,
        Self::Predicate,
        Self::Scan,
        Self::Io,
        Self::Render,
        Self::TraitSurface,
        Self::Entry,
        Self::Other,
    ];

    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Constructor => "constructor",
            Self::Getter => "getter",
            Self::Setter => "setter",
            Self::Predicate => "predicate",
            Self::Scan => "scan",
            Self::Io => "io",
            Self::Render => "render",
            Self::TraitSurface => "trait_surface",
            Self::Entry => "entry",
            Self::Other => "other",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "constructor" => Some(Self::Constructor),
            "getter" => Some(Self::Getter),
            "setter" => Some(Self::Setter),
            "predicate" => Some(Self::Predicate),
            "scan" => Some(Self::Scan),
            "io" => Some(Self::Io),
            "render" => Some(Self::Render),
            "trait_surface" => Some(Self::TraitSurface),
            "entry" => Some(Self::Entry),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

impl Display for FunctionRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// Body complexity, orthogonal to [`FunctionRole`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FunctionComplexity {
    Trivial,
    Linear,
    Branchy,
    Fallible,
    Hotspot,
}

impl FunctionComplexity {
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trivial => "trivial",
            Self::Linear => "linear",
            Self::Branchy => "branchy",
            Self::Fallible => "fallible",
            Self::Hotspot => "hotspot",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "trivial" => Some(Self::Trivial),
            "linear" => Some(Self::Linear),
            "branchy" => Some(Self::Branchy),
            "fallible" => Some(Self::Fallible),
            "hotspot" => Some(Self::Hotspot),
            _ => None,
        }
    }
}

impl Display for FunctionComplexity {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// `tracing` subscriber level used in a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstrumentLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

impl InstrumentLevel {
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            _ => None,
        }
    }
}

impl Display for InstrumentLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// Inputs the per-role recipe strategies read.
#[derive(Debug, Clone)]
#[allow(dead_code)] // returns_self / body_lines feed later delta-rule strategies
pub struct FnContext {
    pub role: FunctionRole,
    pub complexity: FunctionComplexity,
    pub param_names: Vec<String>,
    /// Params whose types cannot be recorded (`impl Trait`, `dyn Trait`, fn generics).
    pub unrecordable_params: Vec<String>,
    pub returns_result: bool,
    pub returns_self: bool,
    pub return_unrecordable: bool,
    pub body_lines: u32,
    pub has_error_path_event: bool,
}

/// Target `#[instrument]` shape for a classified function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecipe {
    pub level: InstrumentLevel,
    pub skip: Vec<String>,
    pub fields: Vec<String>,
    pub err: Option<InstrumentLevel>,
    pub ret: bool,
}

impl InstrumentRecipe {
    /// Render the target attribute apply should write.
    #[instrument(level = "trace", skip(self))]
    pub fn as_attribute(&self) -> String {
        self.render_attribute("instrument")
    }

    /// Fully qualified form used when `instrument` is already a module name.
    #[instrument(level = "trace", skip(self))]
    pub fn as_path_attribute(&self) -> String {
        self.render_attribute("tracing::instrument")
    }

    /// Crate-rooted form used when `tracing` is already a module name.
    #[instrument(level = "trace", skip(self))]
    pub fn as_crate_path_attribute(&self) -> String {
        self.render_attribute("::tracing::instrument")
    }

    fn render_attribute(&self, name: &str) -> String {
        let mut parts = vec![format!("level = \"{}\"", self.level.as_str())];
        if !self.skip.is_empty() {
            parts.push(format!("skip({})", self.skip.join(", ")));
        }
        if !self.fields.is_empty() {
            let fields = self
                .fields
                .iter()
                .map(|field_name| format!("{field_name} = {field_name}"))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("fields({fields})"));
        }
        if let Some(level) = self.err {
            parts.push(format!("err(level = \"{}\")", level.as_str()));
        }
        if self.ret {
            parts.push("ret".to_string());
        }
        format!("#[{name}({})]", parts.join(", "))
    }
}

impl Display for FunctionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Free => write!(f, "free"),
            Self::InherentMethod => write!(f, "inherent"),
            Self::TraitImplMethod => write!(f, "trait_impl"),
        }
    }
}

/// Rust visibility rendered for reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityLabel {
    Public,
    PubCrate,
    PubSuper,
    PubInPath(String),
    Private,
}

impl Display for VisibilityLabel {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Public => write!(f, "pub"),
            Self::PubCrate => write!(f, "pub(crate)"),
            Self::PubSuper => write!(f, "pub(super)"),
            Self::PubInPath(path) => write!(f, "pub({path})"),
            Self::Private => write!(f, "private"),
        }
    }
}

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
            Self::MissingInstrument => {
                "Public or crate-visible function missing `#[instrument]` (recipe on the finding)"
            }
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
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    fn category(&self) -> &str {
        "tracing"
    }

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
    fn probe(&self) -> &str {
        self.label
    }

    fn label(&self) -> &str {
        self.label
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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

//! Function classification enums and the target instrument recipe.

use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

use tracing::instrument;

/// How a discovered function is categorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionKind {
    /// A function declared outside an implementation.
    Free,
    /// A method on an inherent implementation.
    InherentMethod,
    /// A method implementing a trait.
    TraitImplMethod,
}

/// Use-class for an instrument recipe. Dispatch is a `match` on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FunctionRole {
    /// Creates or converts a value.
    Constructor,
    /// Reads a small property of a value.
    Getter,
    /// Updates a value or its configuration.
    Setter,
    /// Answers a boolean question.
    Predicate,
    /// Traverses or collects input.
    Scan,
    /// Loads, reads, writes, or fetches data.
    Io,
    /// Produces a rendered representation.
    Render,
    /// Supplies an otherwise unclassified trait method.
    TraitSurface,
    /// Starts an operation or command.
    Entry,
    /// Has no more specific recognized role.
    Other,
}

impl FunctionRole {
    /// Every role in classification order.
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

    /// Stable label stored in the IR and rendered in reports.
    #[instrument(level = "debug", skip(self))]
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

    /// Read a role from its stored IR label.
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
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// Body complexity, orthogonal to [`FunctionRole`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FunctionComplexity {
    /// At most two statements without branching or a fallible return.
    Trivial,
    /// Straight-line work without a fallible return.
    Linear,
    /// Contains a conditional or loop.
    Branchy,
    /// Returns a recognized Result contract.
    Fallible,
    /// Meets the configured function inventory size floor.
    Hotspot,
}

impl FunctionComplexity {
    /// Stable label stored in the IR and rendered in reports.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trivial => "trivial",
            Self::Linear => "linear",
            Self::Branchy => "branchy",
            Self::Fallible => "fallible",
            Self::Hotspot => "hotspot",
        }
    }

    /// Read complexity from its stored IR label.
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
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// `tracing` subscriber level used in a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum InstrumentLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

impl InstrumentLevel {
    #[instrument(level = "debug", skip(self))]
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
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// Inputs the per-role recipe strategies read.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct FnContext {
    #[getter(copy)]
    role: FunctionRole,
    #[getter(copy)]
    complexity: FunctionComplexity,
    param_names: Vec<String>,
    /// Params tracing cannot record (`impl Trait`, `dyn`, generics, non-Debug types).
    unrecordable_params: Vec<String>,
    #[getter(copy)]
    returns_result: bool,
    #[getter(copy)]
    return_unrecordable: bool,
    /// `true` when the return type (or a `Result`/`Option` payload) borrows.
    /// `#[instrument(err)]` wraps the body in a closure and cannot return those.
    #[getter(copy)]
    return_borrowed: bool,
    /// `true` when `returns_result` is set and the `Err` payload's type is
    /// positively known to implement `Display` -- `#[instrument(err)]`
    /// renders it via `tracing_core::field::display`, which requires that
    /// bound. `false` (not just "unresolved") whenever this can't be
    /// confirmed from the file's own text, so a missing `Display` never
    /// gets a proposed `err()` that can't compile.
    #[getter(copy)]
    err_is_displayable: bool,
    #[getter(copy)]
    has_error_path_event: bool,
}

impl FnContext {
    /// Start a builder for this value.
    #[instrument(level = "debug")]
    pub fn builder() -> FnContextBuilder {
        FnContextBuilder::default()
    }
}

/// Target `#[instrument]` shape for a classified function.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InstrumentRecipe {
    #[getter(copy)]
    level: InstrumentLevel,
    skip: Vec<String>,
    fields: Vec<String>,
    #[getter(copy)]
    err: Option<InstrumentLevel>,
    #[getter(copy)]
    ret: bool,
}

impl InstrumentRecipe {
    /// Start a builder for this value.
    #[instrument(level = "debug")]
    pub fn builder() -> InstrumentRecipeBuilder {
        InstrumentRecipeBuilder::default()
    }
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

    #[instrument(level = "debug", skip(self))]
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
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Free => write!(f, "free"),
            Self::InherentMethod => write!(f, "inherent"),
            Self::TraitImplMethod => write!(f, "trait_impl"),
        }
    }
}

/// Rust visibility rendered for reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityLabel {
    /// Visible outside the crate.
    Public,
    /// Visible throughout the crate.
    PubCrate,
    /// Visible in the parent module.
    PubSuper,
    /// Restricted to the named module path.
    PubInPath(String),
    /// Uses inherited visibility.
    Private,
}

impl Display for VisibilityLabel {
    #[instrument(level = "trace", skip(self, f))]
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

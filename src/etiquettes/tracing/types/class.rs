//! Function classification enums and the target instrument recipe.

use std::fmt::{Display, Formatter, Result as FmtResult};

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
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

/// Inputs the per-role recipe strategies read.
#[derive(Debug, Clone)]
pub struct FnContext {
    pub role: FunctionRole,
    pub complexity: FunctionComplexity,
    pub param_names: Vec<String>,
    /// Params tracing cannot record (`impl Trait`, `dyn`, generics, non-Debug types).
    pub unrecordable_params: Vec<String>,
    pub returns_result: bool,
    pub return_unrecordable: bool,
    /// `true` when the return type (or a `Result`/`Option` payload) borrows.
    /// `#[instrument(err)]` wraps the body in a closure and cannot return those.
    pub return_borrowed: bool,
    /// `true` when `returns_result` is set and the `Err` payload's type is
    /// positively known to implement `Display` -- `#[instrument(err)]`
    /// renders it via `tracing_core::field::display`, which requires that
    /// bound. `false` (not just "unresolved") whenever this can't be
    /// confirmed from the file's own text, so a missing `Display` never
    /// gets a proposed `err()` that can't compile.
    pub err_is_displayable: bool,
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

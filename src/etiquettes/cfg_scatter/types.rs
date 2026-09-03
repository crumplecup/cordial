use std::collections::BTreeSet;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Stable rule identifier for a scattered-`cfg` finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CfgScatterRuleId {
    /// The same `#[cfg(...)]` predicate is applied to multiple distinct item
    /// kinds in one file, where a single `#[cfg]` on a `mod` declaration
    /// (or a shared context struct) would do.
    Scatter001,
}

impl CfgScatterRuleId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scatter001 => "CFG-SCATTER-001",
        }
    }
}

impl Display for CfgScatterRuleId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// What syntactic unit a `#[cfg(...)]` attribute is attached to.
///
/// [`Self::Field`] and [`Self::Variant`] are deliberately excluded from the
/// scatter signal: gating a struct or enum's fields is often unavoidable
/// once one member holds a feature-gated type, and doesn't indicate logic
/// that should be extracted into its own module. Every other kind gating
/// free-standing code (functions, whole types, imports, match arms, …)
/// counts, because that logic *can* move into a `#[cfg]`-gated module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CfgSiteKind {
    /// field.
    Field,
    /// variant.
    Variant,
    /// fn.
    Fn,
    /// impl_fn.
    ImplFn,
    /// A default method inside a `trait { ... }` body — distinct from
    /// [`Self::ImplFn`] (a method inside an `impl` block) since it lives in
    /// a different syntactic home and is fixed differently.
    TraitFn,
    /// struct.
    Struct,
    /// enum.
    Enum,
    /// trait.
    Trait,
    /// impl.
    Impl,
    /// const.
    Const,
    /// static.
    Static,
    /// type_alias.
    TypeAlias,
    /// use.
    Use,
    /// arm.
    Arm,
}

impl CfgSiteKind {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::Variant => "variant",
            Self::Fn => "fn",
            Self::ImplFn => "impl_fn",
            Self::TraitFn => "trait_fn",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Const => "const",
            Self::Static => "static",
            Self::TypeAlias => "type_alias",
            Self::Use => "use",
            Self::Arm => "arm",
        }
    }

    /// Whether this kind is a field or variant (not a free-standing item).
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_field_like(self) -> bool {
        matches!(self, Self::Field | Self::Variant)
    }
}

impl Display for CfgSiteKind {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Thresholds controlling when a repeated `#[cfg(...)]` predicate in one
/// file counts as scatter worth flagging.
///
/// Numbers live in `cordial.toml` ([`crate::config::CfgScatterThresholds`]).
pub use crate::config::CfgScatterThresholds;

/// One raw `#[cfg(...)]` occurrence collected while scanning a file.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct CfgSiteOccurrence {
    #[getter(copy)]
    kind: CfgSiteKind,
    context: String,
    #[getter(copy)]
    line: u32,
    snippet: String,
}

impl CfgSiteOccurrence {
    /// Start a builder for this value.
    pub fn builder() -> CfgSiteOccurrenceBuilder {
        CfgSiteOccurrenceBuilder::default()
    }
}

/// All `#[cfg(...)]` occurrences in one file that share the same predicate.
/// `mod` declarations are never collected here — gating a whole module is
/// the pattern this lint recommends, not an antipattern.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct CfgScatterGroup {
    file: PathBuf,
    predicate: String,
    occurrences: Vec<CfgSiteOccurrence>,
}

impl CfgScatterGroup {
    #[instrument(level = "trace", skip(self))]
    fn non_field_occurrences(&self) -> impl Iterator<Item = &CfgSiteOccurrence> {
        self.occurrences.iter().filter(|o| !o.kind.is_field_like())
    }

    #[instrument(level = "trace", skip(self))]
    pub fn distinct_non_field_kinds(&self) -> BTreeSet<CfgSiteKind> {
        self.non_field_occurrences().map(|o| o.kind).collect()
    }

    #[instrument(level = "trace", skip(self))]
    pub fn non_field_count(&self) -> usize {
        self.non_field_occurrences().count()
    }

    /// Fields-only gating (any count) never flags — see [`CfgSiteKind`] docs.
    #[instrument(level = "trace", skip(self, thresholds))]
    pub fn is_scatter(&self, thresholds: &CfgScatterThresholds) -> bool {
        let distinct = self.distinct_non_field_kinds();
        !distinct.is_empty()
            && (distinct.len() >= thresholds.min_distinct_kinds()
                || self.non_field_count() >= thresholds.min_occurrences())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct CfgScatterRule {
    rule_id: CfgScatterRuleId,
}

impl Rule for CfgScatterRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "cfg_scatter"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Same #[cfg(...)] predicate scattered across multiple item kinds in one file; \
         extract into a #[cfg]-gated module instead"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct CfgScatterMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for CfgScatterMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "cfg-scatter-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "cfg-scatter-site"
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
pub struct CfgScatterFinding {
    rule: CfgScatterRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    predicate: String,
    span: FileSpan,
    distinct_kinds: Vec<String>,
    #[getter(copy)]
    occurrence_count: usize,
    sample_snippets: Vec<String>,
}

impl CfgScatterFinding {
    /// Start a builder for this value.
    pub fn builder() -> CfgScatterFindingBuilder {
        CfgScatterFindingBuilder::default()
    }
}

impl Finding for CfgScatterFinding {
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
        sink.field("rule_id", &self.rule.rule_id);
        sink.field("predicate", &self.predicate);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("kinds", &self.distinct_kinds.join("+"));
        sink.field("occurrences", &self.occurrence_count.to_string());
        sink.field("sample", &self.sample_snippets.join("; "));
    }
}

/// Raw scan row used while building IR nodes: one per scattered group.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct CfgScatterRecord {
    file: PathBuf,
    predicate: String,
    distinct_kinds: Vec<CfgSiteKind>,
    #[getter(copy)]
    occurrence_count: usize,
    sample_snippets: Vec<String>,
}

impl CfgScatterRecord {
    /// Start a builder for this value.
    pub fn builder() -> CfgScatterRecordBuilder {
        CfgScatterRecordBuilder::default()
    }
}

impl CfgScatterRecord {
    #[instrument(level = "debug", skip(group), err(level = "warn"))]
    pub fn from_group(group: &CfgScatterGroup) -> crate::error::CordialResult<Self> {
        let distinct_kinds: Vec<CfgSiteKind> =
            group.distinct_non_field_kinds().into_iter().collect();
        let sample_snippets = group
            .non_field_occurrences()
            .take(5)
            .map(|o| format!("{}:{} {}", o.context(), o.line(), o.snippet()))
            .collect();
        Self::builder()
            .file(group.file().clone())
            .predicate(group.predicate().clone())
            .distinct_kinds(distinct_kinds)
            .occurrence_count(group.non_field_count())
            .sample_snippets(sample_snippets)
            .build()
    }
}

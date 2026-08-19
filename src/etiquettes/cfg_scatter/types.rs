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
    Field,
    Variant,
    Fn,
    ImplFn,
    /// A default method inside a `trait { ... }` body — distinct from
    /// [`Self::ImplFn`] (a method inside an `impl` block) since it lives in
    /// a different syntactic home and is fixed differently.
    TraitFn,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    Static,
    TypeAlias,
    Use,
    Arm,
}

impl CfgSiteKind {
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
#[derive(Debug, Clone)]
pub struct CfgSiteOccurrence {
    pub kind: CfgSiteKind,
    pub context: String,
    pub line: u32,
    pub snippet: String,
}

/// All `#[cfg(...)]` occurrences in one file that share the same predicate.
/// `mod` declarations are never collected here — gating a whole module is
/// the pattern this lint recommends, not an antipattern.
#[derive(Debug, Clone)]
pub struct CfgScatterGroup {
    pub file: PathBuf,
    pub predicate: String,
    pub occurrences: Vec<CfgSiteOccurrence>,
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
            && (distinct.len() >= thresholds.min_distinct_kinds
                || self.non_field_count() >= thresholds.min_occurrences)
    }
}

#[derive(Debug, Clone)]
pub struct CfgScatterRule {
    pub rule_id: CfgScatterRuleId,
}

impl CfgScatterRule {
    #[instrument(level = "debug", skip(rule_id), ret)]
    pub fn new(rule_id: CfgScatterRuleId) -> Self {
        Self { rule_id }
    }
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

#[derive(Debug, Clone)]
pub struct CfgScatterMarker {
    pub anchor: crate::objects::NodeAnchor,
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

#[derive(Debug, Clone)]
pub struct CfgScatterFinding {
    pub rule: CfgScatterRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub predicate: String,
    pub span: FileSpan,
    pub distinct_kinds: Vec<String>,
    pub occurrence_count: usize,
    pub sample_snippets: Vec<String>,
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
        sink.field("file", &self.span.file.display().to_string());
        sink.field("kinds", &self.distinct_kinds.join("+"));
        sink.field("occurrences", &self.occurrence_count.to_string());
        sink.field("sample", &self.sample_snippets.join("; "));
    }
}

/// Raw scan row used while building IR nodes: one per scattered group.
#[derive(Debug, Clone)]
pub struct CfgScatterRecord {
    pub file: PathBuf,
    pub predicate: String,
    pub distinct_kinds: Vec<CfgSiteKind>,
    pub occurrence_count: usize,
    pub sample_snippets: Vec<String>,
}

impl From<&CfgScatterGroup> for CfgScatterRecord {
    #[instrument(level = "debug", skip(group), ret)]
    fn from(group: &CfgScatterGroup) -> Self {
        let distinct_kinds: Vec<CfgSiteKind> =
            group.distinct_non_field_kinds().into_iter().collect();
        let sample_snippets = group
            .non_field_occurrences()
            .take(5)
            .map(|o| format!("{}:{} {}", o.context, o.line, o.snippet))
            .collect();
        Self {
            file: group.file.clone(),
            predicate: group.predicate.clone(),
            distinct_kinds,
            occurrence_count: group.non_field_count(),
            sample_snippets,
        }
    }
}

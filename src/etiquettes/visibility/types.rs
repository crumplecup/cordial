use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Which visibility rule fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VisibilityRuleId {
    /// Crate public surface is below [`VisibilityThresholds::max_crate_names_for_flat`]
    /// but the crate still has a `pub mod` on a public path.
    CrateFlat001,
    /// A visible module has fewer leaf names than
    /// [`VisibilityThresholds::min_module_names`].
    ModThin001,
    /// A child is `pub` while its parent is not — the "pub mod in a private
    /// mod" hole that splits crate-internal paths.
    ModMismatch001,
}

impl VisibilityRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrateFlat001 => "VIS-CRATE-FLAT-001",
            Self::ModThin001 => "VIS-MOD-THIN-001",
            Self::ModMismatch001 => "VIS-MOD-MISMATCH-001",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "VIS-CRATE-FLAT-001" => Some(Self::CrateFlat001),
            "VIS-MOD-THIN-001" => Some(Self::ModThin001),
            "VIS-MOD-MISMATCH-001" => Some(Self::ModMismatch001),
            _ => None,
        }
    }
}

impl Display for VisibilityRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Thresholds for the visibility etiquette. Numbers live in `cordial.toml`.
pub use crate::config::VisibilityThresholds;

#[derive(Debug, Clone, derive_new::new)]
pub struct VisibilityRule {
    rule_id: VisibilityRuleId,
}

impl Rule for VisibilityRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "visibility"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Public module paths must earn their existence: a small crate stays flat; \
         a visible module needs enough names; a child's vis must not exceed its parent"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct VisibilityMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for VisibilityMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "visibility-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "visibility-site"
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
pub struct VisibilityFinding {
    rule: VisibilityRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    module_path: String,
    span: FileSpan,
    #[getter(copy)]
    name_count: usize,
    parent_vis: String,
    declared_vis: String,
}

impl VisibilityFinding {
    /// Start a builder for this value.
    pub fn builder() -> VisibilityFindingBuilder {
        VisibilityFindingBuilder::default()
    }
}

impl Finding for VisibilityFinding {
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
        sink.field("module_path", &self.module_path);
        sink.field("context", &self.module_path);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("name_count", &self.name_count.to_string());
        sink.field("parent_vis", &self.parent_vis);
        sink.field("declared_vis", &self.declared_vis);
    }
}

/// One visibility finding from the crate-tree scan.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct VisibilityRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: VisibilityRuleId,
    /// Module path this visibility finding refers to.
    module_path: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// How many leaf names this module exposes.
    #[getter(copy)]
    name_count: usize,
    /// Visibility of the parent module.
    parent_vis: String,
    /// Visibility declared on this item.
    declared_vis: String,
}

impl VisibilityRecord {
    /// Start a builder for this value.
    pub fn builder() -> VisibilityRecordBuilder {
        VisibilityRecordBuilder::default()
    }
}

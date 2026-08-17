use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrateFlat001 => "VIS-CRATE-FLAT-001",
            Self::ModThin001 => "VIS-MOD-THIN-001",
            Self::ModMismatch001 => "VIS-MOD-MISMATCH-001",
        }
    }

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
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Thresholds for the visibility etiquette. Numbers live in `cordial.toml`.
pub use crate::config::{VisibilityThresholds, load_visibility_thresholds};

#[derive(Debug, Clone)]
pub struct VisibilityRule {
    pub rule_id: VisibilityRuleId,
}

impl VisibilityRule {
    pub fn new(rule_id: VisibilityRuleId) -> Self {
        Self { rule_id }
    }
}

impl Rule for VisibilityRule {
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    fn category(&self) -> &str {
        "visibility"
    }

    fn description(&self) -> &str {
        "Public module paths must earn their existence: a small crate stays flat; \
         a visible module needs enough names; a child's vis must not exceed its parent"
    }
}

#[derive(Debug, Clone)]
pub struct VisibilityMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for VisibilityMarker {
    fn probe(&self) -> &str {
        "visibility-site"
    }

    fn label(&self) -> &str {
        "visibility-site"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct VisibilityFinding {
    pub rule: VisibilityRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub module_path: String,
    pub span: FileSpan,
    pub name_count: usize,
    pub parent_vis: String,
    pub declared_vis: String,
}

impl Finding for VisibilityFinding {
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
        sink.field("rule_id", &self.rule.rule_id);
        sink.field("module_path", &self.module_path);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("name_count", &self.name_count.to_string());
        sink.field("parent_vis", &self.parent_vis);
        sink.field("declared_vis", &self.declared_vis);
    }
}

/// One visibility finding from the crate-tree scan.
#[derive(Debug, Clone)]
pub struct VisibilityRecord {
    pub rule_id: VisibilityRuleId,
    pub module_path: String,
    pub file: PathBuf,
    pub line: u32,
    pub name_count: usize,
    pub parent_vis: String,
    pub declared_vis: String,
}

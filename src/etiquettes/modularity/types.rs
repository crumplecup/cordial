use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

/// Which modularity metric fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModularityKind {
    File,
    Function,
    TypesPerFile,
    ModuleSize,
    TopHeavy,
    Lopsided,
}

impl ModularityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "MODULARITY-FILE",
            Self::Function => "MODULARITY-FUNCTION",
            Self::TypesPerFile => "MODULARITY-TYPES-PER-FILE",
            Self::ModuleSize => "MODULARITY-MODULE-SIZE",
            Self::TopHeavy => "MODULARITY-TOP-HEAVY",
            Self::Lopsided => "MODULARITY-LOPSIDED",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "MODULARITY-FILE" => Some(Self::File),
            "MODULARITY-FUNCTION" => Some(Self::Function),
            "MODULARITY-TYPES-PER-FILE" => Some(Self::TypesPerFile),
            "MODULARITY-MODULE-SIZE" => Some(Self::ModuleSize),
            "MODULARITY-TOP-HEAVY" => Some(Self::TopHeavy),
            "MODULARITY-LOPSIDED" => Some(Self::Lopsided),
            _ => None,
        }
    }
}

impl Display for ModularityKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Thresholds controlling inventory vs checklist inclusion.
///
/// Numbers live in `cordial.toml` ([`crate::config::ModularityThresholds`]).
pub use crate::config::ModularityThresholds;

impl ModularityThresholds {
    pub fn is_checklist_item(&self, kind: ModularityKind, lines: u32) -> bool {
        match kind {
            ModularityKind::File => lines >= self.file_checklist_min_lines,
            ModularityKind::Function => lines >= self.function_checklist_min_lines,
            ModularityKind::TypesPerFile => true,
            ModularityKind::TopHeavy | ModularityKind::Lopsided => true,
            // Signed z-score in the assessor: upper tail also needs the
            // file inventory floor; lower tail has its own ignore flag.
            ModularityKind::ModuleSize => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModularityRule {
    pub kind: ModularityKind,
}

impl ModularityRule {
    pub fn new(kind: ModularityKind) -> Self {
        Self { kind }
    }
}

impl Rule for ModularityRule {
    fn id(&self) -> &str {
        self.kind.as_str()
    }

    fn category(&self) -> &str {
        "modularity"
    }

    fn description(&self) -> &str {
        "Oversized source file, a function or method body that should be split, \
         too many types in one file, a module whose size is a crate-wide outlier, \
         a parent that kept most of its subtree, or a sibling that dwarfs the rest"
    }
}

#[derive(Debug, Clone)]
pub struct ModularityMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for ModularityMarker {
    fn probe(&self) -> &str {
        "modularity-site"
    }

    fn label(&self) -> &str {
        "modularity-site"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ModularityFinding {
    pub rule: ModularityRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub lines: u32,
    pub checklist: bool,
    pub zscore: Option<f64>,
    pub inline: bool,
    pub share: Option<f64>,
    pub detail: String,
}

impl Finding for ModularityFinding {
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
        sink.field("kind", &self.rule.kind);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("lines", &self.lines.to_string());
        sink.field("checklist", &self.checklist.to_string());
        if let Some(zscore) = self.zscore {
            sink.field("zscore", &format!("{zscore:.2}"));
        }
        if let Some(share) = self.share {
            sink.field("share", &format!("{share:.2}"));
        }
        if !self.detail.is_empty() {
            sink.field("detail", &self.detail);
        }
        if self.rule.kind == ModularityKind::ModuleSize {
            sink.field("inline", &self.inline.to_string());
        }
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone)]
pub struct ModularitySiteRecord {
    pub kind: ModularityKind,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub lines: u32,
    pub inline: bool,
}

/// Sample mean and standard deviation for module (or other) line counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModuleSizeStats {
    pub n: usize,
    pub mean: f64,
    pub stddev: f64,
}

impl ModuleSizeStats {
    pub fn from_lines(lines: &[u32]) -> Self {
        let n = lines.len();
        if n == 0 {
            return Self {
                n: 0,
                mean: 0.0,
                stddev: 0.0,
            };
        }
        let mean = lines.iter().map(|value| f64::from(*value)).sum::<f64>() / n as f64;
        if n < 2 {
            return Self {
                n,
                mean,
                stddev: 0.0,
            };
        }
        let variance = lines
            .iter()
            .map(|value| {
                let delta = f64::from(*value) - mean;
                delta * delta
            })
            .sum::<f64>()
            / (n - 1) as f64;
        Self {
            n,
            mean,
            stddev: variance.sqrt(),
        }
    }

    pub fn zscore(self, lines: u32) -> Option<f64> {
        if self.n < 2 || self.stddev <= 0.0 {
            None
        } else {
            Some((f64::from(lines) - self.mean) / self.stddev)
        }
    }

    pub fn is_outlier(self, lines: u32, sigma: u32) -> bool {
        self.is_upper_outlier(lines, sigma) || self.is_lower_outlier(lines, sigma)
    }

    pub fn is_upper_outlier(self, lines: u32, sigma: u32) -> bool {
        self.zscore(lines)
            .is_some_and(|zscore| zscore > f64::from(sigma))
    }

    pub fn is_lower_outlier(self, lines: u32, sigma: u32) -> bool {
        self.zscore(lines)
            .is_some_and(|zscore| zscore < -f64::from(sigma))
    }
}

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Which modularity metric fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModularityKind {
    /// `MODULARITY-FILE`.
    File,
    /// `MODULARITY-FUNCTION`.
    Function,
    /// `MODULARITY-TYPES-PER-FILE`.
    TypesPerFile,
    /// `MODULARITY-MODULE-SIZE`.
    ModuleSize,
    /// `MODULARITY-TOP-HEAVY`.
    TopHeavy,
    /// `MODULARITY-LOPSIDED`.
    Lopsided,
    /// `MODULARITY-COLLAPSE`.
    Collapse,
}

impl ModularityKind {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "MODULARITY-FILE",
            Self::Function => "MODULARITY-FUNCTION",
            Self::TypesPerFile => "MODULARITY-TYPES-PER-FILE",
            Self::ModuleSize => "MODULARITY-MODULE-SIZE",
            Self::TopHeavy => "MODULARITY-TOP-HEAVY",
            Self::Lopsided => "MODULARITY-LOPSIDED",
            Self::Collapse => "MODULARITY-COLLAPSE",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "MODULARITY-FILE" => Some(Self::File),
            "MODULARITY-FUNCTION" => Some(Self::Function),
            "MODULARITY-TYPES-PER-FILE" => Some(Self::TypesPerFile),
            "MODULARITY-MODULE-SIZE" => Some(Self::ModuleSize),
            "MODULARITY-TOP-HEAVY" => Some(Self::TopHeavy),
            "MODULARITY-LOPSIDED" => Some(Self::Lopsided),
            "MODULARITY-COLLAPSE" => Some(Self::Collapse),
            _ => None,
        }
    }
}

impl Display for ModularityKind {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Thresholds controlling inventory vs checklist inclusion.
///
/// Numbers live in `cordial.toml` ([`crate::config::ModularityThresholds`]).
pub use crate::config::ModularityThresholds;

impl ModularityThresholds {
    /// Whether this kind is large enough to appear on the checklist.
    #[instrument(level = "trace", skip(self, kind))]
    pub fn is_checklist_item(&self, kind: ModularityKind, lines: u32) -> bool {
        match kind {
            ModularityKind::File => lines >= self.file_checklist_min_lines(),
            ModularityKind::Function => lines >= self.function_checklist_min_lines(),
            ModularityKind::TypesPerFile => true,
            ModularityKind::TopHeavy | ModularityKind::Lopsided | ModularityKind::Collapse => true,
            // Signed z-score in the assessor: upper tail also needs the
            // file inventory floor; lower tail has its own ignore flag.
            ModularityKind::ModuleSize => false,
        }
    }

    /// Whether `file` is a configured generated-code exception, exempt
    /// from the file-size and module-size LOC checks (`MODULARITY-FILE`,
    /// `MODULARITY-MODULE-SIZE`). There is no reliable way to detect
    /// "this file is generated" from the source alone, so `cordial.toml`
    /// names known generated targets explicitly under
    /// `[modularity] generated_files`. Matched against `file`'s
    /// crate-relative path as an exact match or a path prefix -- the same
    /// folder-or-file idiom `[tracing.stdio] skip_folders` already uses,
    /// so one entry can name either a single generated file or a whole
    /// directory of them (e.g. a `derived_witness/` tree).
    #[instrument(level = "debug", skip(self, file, crate_root), ret)]
    pub fn is_generated_file(&self, file: &Path, crate_root: &Path) -> bool {
        let rel = file.strip_prefix(crate_root).unwrap_or(file);
        self.generated_files().iter().any(|entry| {
            let prefix = Path::new(entry);
            rel == prefix || rel.starts_with(prefix)
        })
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct ModularityRule {
    kind: ModularityKind,
}

impl Rule for ModularityRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.kind.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "modularity"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Oversized source file, a function or method body that should be split, \
         too many types in one file, a module whose size is a crate-wide outlier, \
         a parent that kept most of its subtree, a sibling that dwarfs the rest, \
         or a unary child directory that adds a hop without a fork"
    }
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct ModularityMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for ModularityMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "modularity-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "modularity-site"
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
pub struct ModularityFinding {
    rule: ModularityRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    #[getter(copy)]
    lines: u32,
    #[getter(copy)]
    checklist: bool,
    #[getter(copy)]
    zscore: Option<f64>,
    #[getter(copy)]
    inline: bool,
    #[getter(copy)]
    share: Option<f64>,
    detail: String,
}

impl ModularityFinding {
    /// Start a builder for this value.
    pub fn builder() -> ModularityFindingBuilder {
        ModularityFindingBuilder::default()
    }
}

impl Finding for ModularityFinding {
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
        sink.field("kind", &self.rule.kind);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
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
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ModularitySiteRecord {
    #[getter(copy)]
    kind: ModularityKind,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    lines: u32,
    #[getter(copy)]
    inline: bool,
}

impl ModularitySiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> ModularitySiteRecordBuilder {
        ModularitySiteRecordBuilder::default()
    }
}

/// Sample mean and standard deviation for module (or other) line counts.
#[derive(Debug, Clone, Copy, PartialEq, derive_new::new, derive_getters::Getters)]
pub struct ModuleSizeStats {
    /// Sample size.
    #[getter(copy)]
    n: usize,
    /// Sample mean.
    #[getter(copy)]
    mean: f64,
    /// Sample standard deviation.
    #[getter(copy)]
    stddev: f64,
}

impl ModuleSizeStats {
    /// Compute mean and standard deviation from a slice of line counts.
    #[instrument(level = "debug", ret)]
    pub fn from_lines(lines: &[u32]) -> Self {
        let n = lines.len();
        if n == 0 {
            return Self::new(0, 0.0, 0.0);
        }
        let mean = lines.iter().map(|value| f64::from(*value)).sum::<f64>() / n as f64;
        if n < 2 {
            return Self::new(n, mean, 0.0);
        }
        let variance = lines
            .iter()
            .map(|value| {
                let delta = f64::from(*value) - mean;
                delta * delta
            })
            .sum::<f64>()
            / (n - 1) as f64;
        Self::new(n, mean, variance.sqrt())
    }

    /// Zscore.
    #[instrument(level = "debug", skip(self))]
    pub fn zscore(self, lines: u32) -> Option<f64> {
        if self.n < 2 || self.stddev <= 0.0 {
            None
        } else {
            Some((f64::from(lines) - self.mean) / self.stddev)
        }
    }

    /// Whether `|z|` exceeds `sigma` on either tail.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_outlier(self, lines: u32, sigma: u32) -> bool {
        self.is_upper_outlier(lines, sigma) || self.is_lower_outlier(lines, sigma)
    }

    /// Whether `z` exceeds `+sigma`.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_upper_outlier(self, lines: u32, sigma: u32) -> bool {
        self.zscore(lines)
            .is_some_and(|zscore| zscore > f64::from(sigma))
    }

    /// Whether `z` is below `-sigma`.
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_lower_outlier(self, lines: u32, sigma: u32) -> bool {
        self.zscore(lines)
            .is_some_and(|zscore| zscore < -f64::from(sigma))
    }
}

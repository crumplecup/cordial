use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;
/// Rule identifier for a manual pattern that should use a derive crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeriveRuleId {
    Builder001,
    UseBuilder001,
    Getter001,
    Setter001,
    AsRef001,
    AsStr001,
    New001,
    PubField001,
}

impl DeriveRuleId {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builder001 => "DERIVE-BUILDER-001",
            Self::UseBuilder001 => "DERIVE-USE-BUILDER-001",
            Self::Getter001 => "DERIVE-GETTER-001",
            Self::Setter001 => "DERIVE-SETTER-001",
            Self::AsRef001 => "DERIVE-ASREF-001",
            Self::AsStr001 => "DERIVE-ASSTR-001",
            Self::New001 => "DERIVE-NEW-001",
            Self::PubField001 => "DERIVE-PUB-FIELD-001",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "DERIVE-BUILDER-001" => Some(Self::Builder001),
            "DERIVE-USE-BUILDER-001" => Some(Self::UseBuilder001),
            "DERIVE-GETTER-001" => Some(Self::Getter001),
            "DERIVE-SETTER-001" => Some(Self::Setter001),
            "DERIVE-ASREF-001" => Some(Self::AsRef001),
            "DERIVE-ASSTR-001" => Some(Self::AsStr001),
            "DERIVE-NEW-001" => Some(Self::New001),
            "DERIVE-PUB-FIELD-001" => Some(Self::PubField001),
            _ => None,
        }
    }
}

impl Display for DeriveRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct DeriveRule {
    rule_id: DeriveRuleId,
}

impl Rule for DeriveRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "derives"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Manual builder, constructor arity, getter, setter, AsRef/as_str, new(), or public struct field"
    }
}

#[derive(Debug, Clone)]
pub struct DeriveMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for DeriveMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "derive-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "derive-site"
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
pub struct DeriveFinding {
    pub rule: DeriveRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub struct_name: String,
    pub method_name: Option<String>,
    pub qualified_name: String,
    pub recommendation: String,
    pub span: FileSpan,
    pub evidence: String,
}

impl Finding for DeriveFinding {
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
        sink.field("struct_name", &self.struct_name);
        sink.field("method_name", &self.method_name.clone().unwrap_or_default());
        sink.field("qualified_name", &self.qualified_name);
        sink.field("recommendation", &self.recommendation);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("evidence", &self.evidence);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone)]
pub struct DeriveSiteRecord {
    pub rule_id: DeriveRuleId,
    pub struct_name: String,
    pub method_name: Option<String>,
    pub qualified_name: String,
    pub recommendation: String,
    pub file: PathBuf,
    pub line: u32,
    pub evidence: String,
}

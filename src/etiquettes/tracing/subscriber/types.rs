use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Stable rule identifier for a tracing-subscriber init-policy finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubscriberRuleId {
    /// Binary `fn main` never calls an init helper.
    Main,
    /// A `#[test]` under `tests/` never calls an init helper.
    Test,
    /// The function that installs the subscriber lives outside the library.
    Lib,
    /// The init helper does not read `RUST_LOG` with a fallback.
    RustLog,
    /// The init helper uses `init()` without `Once` / `OnceLock`.
    Idempotent,
}

impl SubscriberRuleId {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "TRACING-SUBSCRIBER-MAIN",
            Self::Test => "TRACING-SUBSCRIBER-TEST",
            Self::Lib => "TRACING-SUBSCRIBER-LIB",
            Self::RustLog => "TRACING-SUBSCRIBER-RUST-LOG",
            Self::Idempotent => "TRACING-SUBSCRIBER-IDEMPOTENT",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "TRACING-SUBSCRIBER-MAIN" => Some(Self::Main),
            "TRACING-SUBSCRIBER-TEST" => Some(Self::Test),
            "TRACING-SUBSCRIBER-LIB" => Some(Self::Lib),
            "TRACING-SUBSCRIBER-RUST-LOG" => Some(Self::RustLog),
            "TRACING-SUBSCRIBER-IDEMPOTENT" => Some(Self::Idempotent),
            _ => None,
        }
    }

    /// Whether `id` is a tracing-subscriber rule (`TRACING-SUBSCRIBER-*`).
    #[instrument(level = "debug")]
    pub fn is_subscriber_rule(id: &str) -> bool {
        id.starts_with("TRACING-SUBSCRIBER-")
    }

    #[instrument(level = "debug", skip(self))]
    fn description(self) -> &'static str {
        match self {
            Self::Main => {
                "Binary `fn main` never installs a tracing subscriber — call the library helper"
            }
            Self::Test => {
                "`#[test]` in `tests/` never installs a tracing subscriber — call the library helper"
            }
            Self::Lib => {
                "Subscriber init lives outside the library — move it to one documented helper"
            }
            Self::RustLog => {
                "Init helper must read `RUST_LOG` with a fallback (`try_from_default_env` + `unwrap_or*`)"
            }
            Self::Idempotent => {
                "Init helper uses `init()` without `Once`/`OnceLock` — use `try_init()` or wrap in `Once`"
            }
        }
    }
}

impl Display for SubscriberRuleId {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct SubscriberRule {
    rule_id: SubscriberRuleId,
}

impl Rule for SubscriberRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.rule_id.as_str()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "tracing"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        self.rule_id.description()
    }
}

pub const SUBSCRIBER_SITE_LABEL: &str = "tracing-subscriber-site";

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct SubscriberMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for SubscriberMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        SUBSCRIBER_SITE_LABEL
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        SUBSCRIBER_SITE_LABEL
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
pub struct SubscriberFinding {
    rule: SubscriberRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl SubscriberFinding {
    /// Start a builder for this value.
    pub fn builder() -> SubscriberFindingBuilder {
        SubscriberFindingBuilder::default()
    }
}

impl Finding for SubscriberFinding {
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
        sink.field("rule", &self.rule.rule_id);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct SubscriberSiteRecord {
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: SubscriberRuleId,
    /// Qualified name or extra locator for this site.
    context: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Source snippet captured at the site.
    snippet: String,
}

impl SubscriberSiteRecord {
    /// Start a builder for this value.
    pub fn builder() -> SubscriberSiteRecordBuilder {
        SubscriberSiteRecordBuilder::default()
    }
}

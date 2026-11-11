use crate::exceptions::StaleException;
use crate::ir::NodeId;
use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, NodeAnchor, Rule};

use tracing::instrument;

/// Rule id for an exception row that suppresses nothing.
pub const STALE_EXCEPTION_RULE_ID: &str = "EXCEPTION-STALE-001";

/// The one rule of the stale-exceptions etiquette.
#[derive(Debug, Clone, Copy, Default)]
pub struct StaleExceptionRule;

impl crate::TomlConfigured for StaleExceptionRule {
    #[instrument(level = "trace", skip(self))]
    fn configuration_key(&self) -> &str {
        "stale_exceptions"
    }
}

impl Rule for StaleExceptionRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        STALE_EXCEPTION_RULE_ID
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "An exception row matches no finding of its etiquette"
    }
}

/// An exception row that no longer suppresses anything.
///
/// Exceptions live in the store, outside the IR, so this finding carries no
/// graph node; its anchor is a placeholder nothing reads.
#[derive(Debug, Clone)]
pub struct StaleExceptionFinding {
    rule: StaleExceptionRule,
    anchor: NodeAnchor,
    crate_name: String,
    stale: StaleException,
}

impl StaleExceptionFinding {
    /// Wrap a stale row found while assessing `crate_name`.
    #[instrument(level = "debug", skip(stale), fields(crate_name = crate_name))]
    pub fn new(crate_name: &str, stale: StaleException) -> Self {
        Self {
            rule: StaleExceptionRule,
            anchor: NodeAnchor::new(NodeId::new(0)),
            crate_name: crate_name.to_string(),
            stale,
        }
    }
}

impl Finding for StaleExceptionFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        Disposition::Open
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn FindingSink) {
        let entry = self.stale.entry();
        let line = entry
            .line()
            .map(|line| line.to_string())
            .unwrap_or_default();
        sink.field("crate", &self.crate_name);
        sink.field("rule_id", &STALE_EXCEPTION_RULE_ID);
        sink.field("etiquette", self.stale.etiquette());
        sink.field("file", entry.file());
        sink.field("line", &line);
        sink.field(
            "expected_rule",
            &entry.rule_id().as_deref().unwrap_or_default(),
        );
        sink.field("context", &entry.context().as_deref().unwrap_or_default());
        sink.field("reason", entry.reason());
        sink.snippet(entry.reason());
    }
}

use crate::objects::{Disposition, FileSpan, Finding, FindingSink, IrAnchor, Rule};

use super::{InternalErrorChainRule, InternalErrorNodeClass, InternalErrorRecordKind};

use tracing::instrument;
#[derive(Debug, Clone)]
pub struct InternalErrorChainFinding {
    pub rule: InternalErrorChainRule,
    pub record_kind: InternalErrorRecordKind,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
    pub type_path: Option<String>,
    pub node_class: Option<InternalErrorNodeClass>,
    pub source_target: Option<String>,
    pub reaches_foreign: Option<bool>,
    pub chain_depth: Option<u32>,
    pub foreign_error_type: Option<String>,
    pub internal_constructor: Option<String>,
}

impl Finding for InternalErrorChainFinding {
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
        sink.field("record_kind", &self.record_kind.as_str());
        sink.field("rule_id", &self.rule.id());
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        if let Some(type_path) = &self.type_path {
            sink.field("type_path", type_path);
        }
        if let Some(node_class) = self.node_class {
            sink.field("node_class", &node_class.to_string());
        }
        sink.field(
            "source_target",
            &self.source_target.clone().unwrap_or_default(),
        );
        if let Some(reaches_foreign) = self.reaches_foreign {
            sink.field("reaches_foreign", &reaches_foreign.to_string());
        }
        if let Some(chain_depth) = self.chain_depth {
            sink.field("chain_depth", &chain_depth.to_string());
        }
        sink.field(
            "foreign_error_type",
            &self.foreign_error_type.clone().unwrap_or_default(),
        );
        sink.field(
            "internal_constructor",
            &self.internal_constructor.clone().unwrap_or_default(),
        );
        sink.snippet(&self.snippet);
    }
}

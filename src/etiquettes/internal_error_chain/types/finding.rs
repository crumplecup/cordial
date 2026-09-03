use crate::objects::{Disposition, FileSpan, Finding, FindingSink, IrAnchor, Rule, SourceSpan};

use super::{InternalErrorChainRule, InternalErrorNodeClass, InternalErrorRecordKind};

use tracing::instrument;
#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InternalErrorChainFinding {
    rule: InternalErrorChainRule,
    #[getter(copy)]
    record_kind: InternalErrorRecordKind,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
    type_path: Option<String>,
    #[getter(copy)]
    node_class: Option<InternalErrorNodeClass>,
    source_target: Option<String>,
    #[getter(copy)]
    reaches_foreign: Option<bool>,
    #[getter(copy)]
    chain_depth: Option<u32>,
    foreign_error_type: Option<String>,
    internal_constructor: Option<String>,
}

impl InternalErrorChainFinding {
    /// Start a builder for this value.
    pub fn builder() -> InternalErrorChainFindingBuilder {
        InternalErrorChainFindingBuilder::default()
    }
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
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        if let Some(type_path) = self.type_path() {
            sink.field("type_path", type_path);
        }
        if let Some(node_class) = self.node_class() {
            sink.field("node_class", &node_class.to_string());
        }
        sink.field(
            "source_target",
            &self.source_target().clone().unwrap_or_default(),
        );
        if let Some(reaches_foreign) = self.reaches_foreign() {
            sink.field("reaches_foreign", &reaches_foreign.to_string());
        }
        if let Some(chain_depth) = self.chain_depth() {
            sink.field("chain_depth", &chain_depth.to_string());
        }
        sink.field(
            "foreign_error_type",
            &self.foreign_error_type().clone().unwrap_or_default(),
        );
        sink.field(
            "internal_constructor",
            &self.internal_constructor().clone().unwrap_or_default(),
        );
        sink.snippet(&self.snippet);
    }
}

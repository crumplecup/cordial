use crate::objects::{Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, SourceSpan};

use super::rule::DependencyFreshnessRule;

use tracing::instrument;

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct DependencyFreshnessMarker {
    anchor: crate::objects::NodeAnchor,
}

impl Marker for DependencyFreshnessMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "dependency-freshness-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "dependency-freshness-site"
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
pub struct DependencyFreshnessFinding {
    rule: DependencyFreshnessRule,
    #[getter(copy)]
    disposition: Disposition,
    anchor: crate::objects::NodeAnchor,
    crate_name: String,
    dependency_name: String,
    package_name: String,
    span: FileSpan,
    section: String,
    version_spec: String,
    locked_versions: String,
    available_versions: String,
    update_kinds: String,
    snippet: String,
}

impl DependencyFreshnessFinding {
    #[instrument(level = "debug")]
    pub fn builder() -> DependencyFreshnessFindingBuilder {
        DependencyFreshnessFindingBuilder::default()
    }
}

impl Finding for DependencyFreshnessFinding {
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn crate::objects::Rule {
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
        sink.field("rule_id", &self.rule.rule_id().as_str());
        sink.field("dependency", &self.dependency_name);
        sink.field("package", &self.package_name);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("section", &self.section);
        sink.field("version_spec", &self.version_spec);
        sink.field("locked_versions", &self.locked_versions);
        sink.field("available_versions", &self.available_versions);
        sink.field("update_kinds", &self.update_kinds);
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

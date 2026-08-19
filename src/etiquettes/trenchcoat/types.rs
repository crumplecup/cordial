use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan};

use tracing::instrument;
#[derive(Debug, Clone, Copy)]
pub struct TrenchcoatRule;

impl Rule for TrenchcoatRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "TRENCHCOAT-MISSING-WRAP"
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "trenchcoat"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Foreign type lacks a trenchcoat wrapper"
    }
}

#[derive(Debug, Clone)]
pub struct UnwrappedMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for UnwrappedMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "unwrapped-foreign"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "unwrapped-foreign"
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
pub struct UnwrappedFinding {
    pub rule: TrenchcoatRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub type_path: String,
}

impl Finding for UnwrappedFinding {
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
        sink.field("kind", &self.rule().id());
        sink.field("context", &self.type_path);
        sink.field("type_path", &self.type_path);
    }
}

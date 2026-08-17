use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan};

#[derive(Debug, Clone, Copy)]
pub struct TrenchcoatRule;

impl Rule for TrenchcoatRule {
    fn id(&self) -> &str {
        "TRENCHCOAT-MISSING-WRAP"
    }

    fn category(&self) -> &str {
        "trenchcoat"
    }

    fn description(&self) -> &str {
        "Foreign type lacks a trenchcoat wrapper"
    }
}

#[derive(Debug, Clone)]
pub struct UnwrappedMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for UnwrappedMarker {
    fn probe(&self) -> &str {
        "unwrapped-foreign"
    }

    fn label(&self) -> &str {
        "unwrapped-foreign"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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
        sink.field("kind", &self.rule().id());
        sink.field("context", &self.type_path);
        sink.field("type_path", &self.type_path);
    }
}

use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan};

use tracing::instrument;
#[derive(Debug, Clone, Copy)]
pub struct ShadowRule;

impl Rule for ShadowRule {
    fn id(&self) -> &str {
        "SHADOW-MISSING-MIRROR"
    }

    fn category(&self) -> &str {
        "shadow"
    }

    fn description(&self) -> &str {
        "Upstream item lacks a shadow mirror link"
    }
}

#[derive(Debug, Clone)]
pub struct MissingMirrorMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for MissingMirrorMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "missing-shadow-mirror"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "missing-shadow-mirror"
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
pub struct MissingMirrorFinding {
    pub rule: ShadowRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub target_path: String,
    pub shadow_path: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowPairRule;

impl Rule for ShadowPairRule {
    fn id(&self) -> &str {
        "SHADOW-PAIR"
    }

    fn category(&self) -> &str {
        "shadow-pair"
    }

    fn description(&self) -> &str {
        "Cross-crate upstream ↔ shadow mirror coverage row"
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowPairChecklistRule;

impl Rule for ShadowPairChecklistRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "SHADOW-PAIR-CHECKLIST"
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "shadow-pair-checklist"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Cross-crate shadow method/trait checklist for one upstream ↔ shadow pair"
    }
}

#[derive(Debug, Clone)]
pub struct ShadowMethodChecklistFinding {
    pub rule: ShadowPairChecklistRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub target_crate: String,
    pub shadow_crate: String,
    pub body: String,
}

impl Finding for ShadowMethodChecklistFinding {
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
        sink.field("target_crate", &self.target_crate);
        sink.field("shadow_crate", &self.shadow_crate);
        sink.field("body", &self.body);
    }
}

#[derive(Debug, Clone)]
pub struct CrossCrateShadowFinding {
    pub rule: ShadowPairRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub target_crate: String,
    pub shadow_crate: String,
    pub row: crate::shadow::ShadowRow,
    pub coverage_pct: f64,
}

impl Finding for CrossCrateShadowFinding {
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
        let render = crate::shadow::render_shadow_row(&self.row);
        sink.field("target_crate", &self.target_crate);
        sink.field("shadow_crate", &self.shadow_crate);
        sink.field("item_path", &self.row.item_path);
        sink.field("item_kind", &self.row.item_kind.as_str());
        sink.field("status", &self.row.status.as_str());
        sink.field("coverage_kind", &render.coverage_kind);
        sink.field("primary_gap_kind", &render.primary_gap_kind);
        sink.field("shadow_item", &self.row.shadow_item);
        sink.field("drift_confidence", &self.row.drift_confidence);
        sink.field("shadow_elicit_impl", &self.row.shadow_elicit_impl);
        let verification_gap = if render.verification_gap {
            "true"
        } else {
            "false"
        };
        let verification_ready = if render.verification_ready {
            "true"
        } else {
            "false"
        };
        sink.field("verification_gap", &verification_gap);
        sink.field("verification_ready", &verification_ready);
        sink.field("shadow_can_be_direct", &self.row.shadow_can_be_direct);
        sink.field(
            "shadow_missing_external_traits",
            &self.row.shadow_missing_external_traits,
        );
        sink.field(
            "shadow_missing_our_traits",
            &self.row.shadow_missing_our_traits,
        );
        sink.field("action", &render.action);
        sink.field("notes", &self.row.notes);
        sink.field("coverage_pct", &format!("{:.1}", self.coverage_pct));
    }
}

impl Finding for MissingMirrorFinding {
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
        sink.field("context", &self.target_path);
        sink.field("target_path", &self.target_path);
        sink.field("shadow_path", &self.shadow_path);
    }
}

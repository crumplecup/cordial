use crate::objects::{Disposition, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan};

use tracing::instrument;
/// Why an impl-coverage item is counted as a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplGapKind {
    /// MissingOurTraits.
    MissingOurTraits,
    /// ReadyForElicitComplete.
    ReadyForElicitComplete,
    /// FeatureGatedExternal.
    FeatureGatedExternal,
    /// ExternallyBlocked.
    ExternallyBlocked,
}

impl ImplGapKind {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingOurTraits => "MissingOurTraits",
            Self::ReadyForElicitComplete => "ReadyForElicitComplete",
            Self::FeatureGatedExternal => "FeatureGatedExternal",
            Self::ExternallyBlocked => "ExternallyBlocked",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CoverageRule;

impl Rule for CoverageRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "IMPL-COVERAGE-GAP"
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "impl-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Type lacks ElicitComplete coverage"
    }
}

#[derive(Debug, Clone)]
pub struct ImplGapMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for ImplGapMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "impl-coverage-gap"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "impl-coverage-gap"
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
pub struct ImplGapFinding {
    pub rule: CoverageRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub type_path: String,
    pub gap_kind: Option<ImplGapKind>,
    pub missing_our_traits: String,
    pub missing_external_traits: String,
    pub elicit_complete_gap: bool,
    pub proof_test: String,
    pub composition_test: String,
    pub feature_gated_external: bool,
    pub feature_owner_crate: String,
    pub candidate_unlock_features: String,
    pub coverage_provider: String,
    pub wrapper_paths: String,
    pub covered_indirectly: bool,
}

impl Finding for ImplGapFinding {
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
        sink.field("kind", &self.gap_kind_label());
        sink.field("context", &self.type_path);
        sink.field("type_path", &self.type_path);
        sink.field("gap_kind", &self.gap_kind_label());
        sink.field("missing_our_traits", &self.missing_our_traits);
        sink.field("missing_external_traits", &self.missing_external_traits);
        sink.field(
            "elicit_complete_gap",
            if self.elicit_complete_gap {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field("proof_test", &self.proof_test);
        sink.field("composition_test", &self.composition_test);
        sink.field(
            "feature_gated_external",
            if self.feature_gated_external {
                &"true"
            } else {
                &"false"
            },
        );
        sink.field("feature_owner_crate", &self.feature_owner_crate);
        sink.field("candidate_unlock_features", &self.candidate_unlock_features);
        sink.field("coverage_provider", &self.coverage_provider);
        sink.field("wrapper_paths", &self.wrapper_paths);
        sink.field(
            "covered_indirectly",
            if self.covered_indirectly {
                &"true"
            } else {
                &"false"
            },
        );
    }
}

impl ImplGapFinding {
    #[instrument(level = "trace", skip(self))]
    fn gap_kind_label(&self) -> String {
        self.gap_kind
            .map(ImplGapKind::as_str)
            .unwrap_or_default()
            .to_string()
    }
}

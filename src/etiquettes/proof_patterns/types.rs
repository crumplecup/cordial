use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

use tracing::instrument;

/// Category of soundness/proof-visibility signal detected in a real
/// `verus! { .. }` function -- see this etiquette's own module doc for
/// what each one means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProofPatternKind {
    Assume,
    Admit,
    ExternalBody,
    Uninterp,
    Axiom,
    Broadcast,
}

impl ProofPatternKind {
    #[instrument(level = "debug", skip(self))]
    pub fn rule_id(self) -> &'static str {
        match self {
            Self::Assume => "PROOF-PATTERN-ASSUME",
            Self::Admit => "PROOF-PATTERN-ADMIT",
            Self::ExternalBody => "PROOF-PATTERN-EXTERNAL-BODY",
            Self::Uninterp => "PROOF-PATTERN-UNINTERP",
            Self::Axiom => "PROOF-PATTERN-AXIOM",
            Self::Broadcast => "PROOF-PATTERN-BROADCAST",
        }
    }

    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "assume" => Some(Self::Assume),
            "admit" => Some(Self::Admit),
            "external_body" => Some(Self::ExternalBody),
            "uninterp" => Some(Self::Uninterp),
            "axiom" => Some(Self::Axiom),
            "broadcast" => Some(Self::Broadcast),
            _ => None,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Assume => "assume",
            Self::Admit => "admit",
            Self::ExternalBody => "external_body",
            Self::Uninterp => "uninterp",
            Self::Axiom => "axiom",
            Self::Broadcast => "broadcast",
        }
    }

    /// Why this signal matters, specific to the kind -- these are not
    /// interchangeable (a `broadcast` lemma is a visibility problem, not
    /// a trust problem; the other five are real, local soundness escape
    /// hatches).
    #[instrument(level = "debug", skip(self))]
    pub fn description(self) -> &'static str {
        match self {
            Self::Assume => {
                "assume(..) trusts a claim instead of proving it -- everything built \
                 on this holds only if the assumption is true"
            }
            Self::Admit => {
                "admit() discharges the entire remaining proof obligation \
                 unconditionally -- the strongest local soundness escape hatch Verus has"
            }
            Self::ExternalBody => {
                "#[verifier::external_body] -- Verus never checks this body against its \
                 own signature; ensures is trusted based on unverified exec code alone"
            }
            Self::Uninterp => {
                "uninterp spec fn has no body -- nothing backs its meaning except the \
                 requires/ensures a caller chooses to trust"
            }
            Self::Axiom => {
                "axiom fn is assumed, not proven -- every proof built on this rests on \
                 it being true, not on anything Verus itself checked"
            }
            Self::Broadcast => {
                "broadcast proof fn applies automatically to every proof in scope via \
                 use -- its contribution to the total proof burden is invisible at call sites"
            }
        }
    }
}

impl Display for ProofPatternKind {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.rule_id())
    }
}

#[derive(Debug, Clone, derive_new::new)]
pub struct ProofPatternRule {
    kind: ProofPatternKind,
}

impl Rule for ProofPatternRule {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.kind.rule_id()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "proof_patterns"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        self.kind.description()
    }
}

/// Marker emitted by [`super::probe::ProofPatternSiteProbe`].
#[derive(Debug, Clone)]
pub struct ProofPatternMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for ProofPatternMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        "proof-pattern-site"
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        "proof-pattern-site"
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

/// Assessed proof-pattern finding.
#[derive(Debug, Clone)]
pub struct ProofPatternFinding {
    pub rule: ProofPatternRule,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
    pub cfg_test: bool,
    pub tracked_params: Vec<String>,
    pub recommends: Vec<String>,
}

impl Finding for ProofPatternFinding {
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
        sink.field("kind", &self.rule.kind);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.field("cfg_test", &self.cfg_test.to_string());
        sink.field("tracked_params", &self.tracked_params.join(", "));
        sink.field("recommends", &self.recommends.join(", "));
        sink.snippet(&self.snippet);
    }
}

/// Raw scan row used while building IR nodes.
#[derive(Debug, Clone)]
pub struct ProofPatternRecord {
    pub kind: ProofPatternKind,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
    pub cfg_test: bool,
    pub tracked_params: Vec<String>,
    pub recommends: Vec<String>,
}

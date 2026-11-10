//! Candidate-independent conversion contracts, projected by the source loader.
use super::{ErrorEdgeEvidence, ErrorFromEvidence, ErrorSourceMethodEvidence, ErrorTryEvidence};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod attributes;
mod collect;
mod select;
mod signatures;

/// Ordered conversion events for one required source snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(crate) struct ConversionFileFacts {
    events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
struct Event {
    start: (usize, usize),
    end: (usize, usize),
    depth: usize,
    owner: Option<String>,
    /// A matched associated call visits arguments but not its callee subtree.
    callee_guards: Vec<CalleeGuard>,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
struct CalleeGuard {
    path: PathFact,
    owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
struct PathFact {
    names: Vec<String>,
    bare: bool,
    qualified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Record {
        path: PathFact,
        rest: bool,
    },
    Caller(PathFact),
    Associated(PathFact),
    Field(String),
    Pattern {
        path: PathFact,
        fields: BTreeSet<String>,
    },
    Try {
        returns: Option<BTreeSet<String>>,
        evidence: Option<ErrorTryEvidence>,
    },
    Type(PathFact),
    Alias {
        names: BTreeSet<String>,
        stable_error: Option<String>,
    },
    Import {
        names: BTreeSet<String>,
        glob: bool,
    },
    Macro {
        constructs: BTreeSet<String>,
        fields: BTreeSet<String>,
        unhandled: BTreeSet<String>,
    },
    From {
        mentions: BTreeSet<String>,
        evidence: ErrorFromEvidence,
    },
    Source(ErrorSourceMethodEvidence),
    Attribute {
        name: String,
        stable_file: bool,
    },
    Edge {
        name: String,
        evidence: Option<ErrorEdgeEvidence>,
    },
}

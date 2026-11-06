//! Located getter consumers and their proven declaration identities in the IR.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::FileSnapshot;

mod identity;
mod native;
mod source;
mod visit;

use identity::Identities;

/// Candidate-independent receiver evidence, collected before crate-wide joins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetterConsumerFacts {
    methods: Vec<MethodContract>,
    uses: Vec<ConsumerSite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MethodContract {
    owner: ReceiverRecipe,
    has_receiver: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum ReceiverRecipe {
    Unknown,
    Location,
    Declaration(String),
    Path(Vec<String>),
    Method {
        receiver: Box<Self>,
        associated: bool,
    },
}

impl ReceiverRecipe {
    #[instrument(level = "trace", skip(self))]
    fn method(self, associated: bool) -> Self {
        Self::Method {
            receiver: Box::new(self),
            associated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConsumerSite {
    line: usize,
    column: usize,
    receiver: ReceiverRecipe,
    kind: UseKind,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct TypeIdentity {
    declaration: PathBuf,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Receiver {
    /// Standard panic locations have an invariant filename API.
    Location,
    /// An inherent or known generated method belongs to this declaration.
    Declaration(TypeIdentity),
    /// Missing type, signature, macro grammar, or resolution evidence.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum UseKind {
    Getter,
    Field,
    Macro,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct GetterUseFact {
    path: PathBuf,
    line: usize,
    column: usize,
    receiver: Receiver,
    kind: UseKind,
    end_line: usize,
    end_column: usize,
}

impl GetterUseFact {
    /// A proven field receiver belongs to another declaration; unknown stays open.
    #[instrument(level = "trace", skip(self, path, start))]
    pub(super) fn unrelated_field(
        &self,
        path: &Path,
        owner: &str,
        usage: &Path,
        start: (usize, usize),
    ) -> bool {
        self.path == usage
            && self.kind == UseKind::Field
            && self.line == start.0
            && self.column == start.1
            && matches!(&self.receiver, Receiver::Declaration(identity)
                if identity.declaration != path || identity.name != owner)
    }

    #[instrument(level = "trace", skip(self, path))]
    pub(super) fn blocks(&self, path: &Path, owner: &str) -> bool {
        if self.kind != UseKind::Getter {
            return false;
        }
        match &self.receiver {
            Receiver::Location => false,
            Receiver::Declaration(identity) => {
                identity.declaration == path && identity.name == owner
            }
            Receiver::Unknown => true,
        }
    }
}

#[instrument(level = "debug", skip(files, source), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn collect(
    files: &[FileSnapshot],
    source: &crate::ir::SourceErrorApply,
) -> crate::CordialResult<Vec<GetterUseFact>> {
    let identities = Identities::collect(files, source)?;
    Ok(files
        .iter()
        .map(|file| {
            Ok(source
                .getter_uses_for(file)?
                .uses
                .iter()
                .map(|site| GetterUseFact {
                    path: file.path().to_path_buf(),
                    line: site.line,
                    column: site.column,
                    receiver: identities.receiver(file, &site.receiver),
                    kind: site.kind,
                    end_line: site.end_line,
                    end_column: site.end_column,
                })
                .collect::<Vec<_>>())
        })
        .collect::<crate::CordialResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect())
}

#[instrument(level = "trace", skip(uses, path, usage, start))]
pub(super) fn unrelated_field(
    uses: &[GetterUseFact],
    path: &Path,
    owner: &str,
    usage: &Path,
    start: (usize, usize),
) -> bool {
    uses.iter()
        .any(|fact| fact.unrelated_field(path, owner, usage, start))
}

#[instrument(level = "trace", skip(uses, path, usage, start, end))]
pub(super) fn unrelated_macro(
    uses: &[GetterUseFact],
    path: &Path,
    owner: &str,
    usage: &Path,
    start: (usize, usize),
    end: (usize, usize),
) -> bool {
    let known = uses.iter().any(|fact| {
        fact.path == usage
            && fact.kind == UseKind::Macro
            && (fact.line, fact.column) == start
            && (fact.end_line, fact.end_column) == end
    });
    known
        && uses
            .iter()
            .filter(|fact| {
                fact.path == usage
                    && fact.kind != UseKind::Macro
                    && (fact.line, fact.column) >= start
                    && (fact.end_line, fact.end_column) <= end
            })
            .all(|fact| match &fact.receiver {
                Receiver::Location => fact.kind == UseKind::Getter,
                Receiver::Declaration(identity) => {
                    identity.declaration != path || identity.name != owner
                }
                Receiver::Unknown => false,
            })
}

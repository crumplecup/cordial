//! Located getter consumers and their proven declaration identities in the IR.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::loader::SharedParsedFile;

mod identity;
mod visit;

use identity::Identities;

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
    #[instrument(level = "trace", skip(self, path, span))]
    pub(super) fn unrelated_field(
        &self,
        path: &Path,
        owner: &str,
        usage: &Path,
        span: proc_macro2::Span,
    ) -> bool {
        self.path == usage
            && self.kind == UseKind::Field
            && self.line == span.start().line
            && self.column == span.start().column
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

#[instrument(level = "debug", skip(files))]
#[hotpath::measure]
pub(super) fn collect(files: &[SharedParsedFile]) -> Vec<GetterUseFact> {
    let identities = Identities::collect(files);
    files
        .iter()
        .flat_map(|file| visit::collect(file, &identities))
        .collect()
}

#[instrument(level = "trace", skip(uses, path, usage, span))]
pub(super) fn unrelated_field(
    uses: &[GetterUseFact],
    path: &Path,
    owner: &str,
    usage: &Path,
    span: proc_macro2::Span,
) -> bool {
    uses.iter()
        .any(|fact| fact.unrelated_field(path, owner, usage, span))
}

#[instrument(level = "trace", skip(uses, path, usage, span))]
pub(super) fn unrelated_macro(
    uses: &[GetterUseFact],
    path: &Path,
    owner: &str,
    usage: &Path,
    span: proc_macro2::Span,
) -> bool {
    let start = (span.start().line, span.start().column);
    let end = (span.end().line, span.end().column);
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

//! Memoized grammar projections for Rust islands in opaque macro token trees.
//!
//! File parsing leaves token trees opaque. These queries parse each supported
//! payload once, with ownership in the source database, rather than in a lint.
//! Recognizing Rust syntax does not establish a macro's expansion semantics.

use super::{super::database::SemanticDatabase, DatabaseSource};
use ra_ap_syntax::{Parse, ast};
use std::collections::HashMap;

#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "antipatterns"
))]
mod forest;
#[cfg(feature = "internal_error_chain")]
mod getters;
mod grammar;
#[cfg(feature = "panics")]
mod panic;

#[salsa::interned]
pub(super) struct Payload<'db> {
    text: String,
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn expression<'db>(
    db: &'db dyn SemanticDatabase,
    key: Payload<'db>,
) -> Parse<ast::Expr> {
    ast::Expr::parse(key.text(db), ra_ap_syntax::Edition::CURRENT)
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn program<'db>(
    db: &'db dyn SemanticDatabase,
    key: Payload<'db>,
) -> Parse<ast::SourceFile> {
    tracing::debug!(target: "cordial::semantic::queries", function = "payload", query = "rust_program", "semantic query executed");
    ast::SourceFile::parse(key.text(db), ra_ap_syntax::Edition::CURRENT)
}

/// Loader-issued syntax only. Lookups use canonical AST identity; a foreign
/// parse cannot add evidence or cause another payload to be parsed.
#[derive(Default)]
pub(crate) struct DatabaseMacroSyntax {
    arguments: HashMap<ast::MacroCall, Vec<ast::Expr>>,
    items: HashMap<ast::MacroCall, Vec<ast::Item>>,
    #[cfg(feature = "internal_error_chain")]
    getter_offsets: HashMap<ast::MacroCall, usize>,
}

impl DatabaseMacroSyntax {
    /// Relative grammar origin, preserving locations when identical memoized
    /// payloads occur at different call sites or inside other payloads.
    #[cfg(feature = "internal_error_chain")]
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn getter_payload(
        &self,
        invocation: &ast::MacroCall,
    ) -> Option<(usize, &[ast::Expr], &[ast::Item])> {
        Some((
            *self.getter_offsets.get(invocation)?,
            self.arguments
                .get(invocation)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            self.items
                .get(invocation)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        ))
    }
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn arguments(&self, invocation: &ast::MacroCall) -> &[ast::Expr] {
        self.arguments
            .get(invocation)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[cfg(any(feature = "tracing", feature = "panics"))]
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn items(&self, invocation: &ast::MacroCall) -> &[ast::Item] {
        self.items
            .get(invocation)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

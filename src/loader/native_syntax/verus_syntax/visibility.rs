//! Visibility counts reuse the canonical dialect payload query.
use super::DatabaseSource;
use ra_ap_syntax::{AstNode, ast};
use std::collections::HashMap;

/// Canonical invocation identities prevent substitution of a separately parsed macro.
#[derive(Default)]
pub(crate) struct DatabaseVerusSyntax {
    names: HashMap<ast::MacroCall, (usize, usize)>,
}

impl DatabaseVerusSyntax {
    #[tracing::instrument(level = "trace", skip(self, invocation))]
    pub(crate) fn visible_names(&self, invocation: &ast::MacroCall) -> (usize, usize) {
        self.names.get(invocation).copied().unwrap_or_default()
    }
}

impl DatabaseSource<'_> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn verus_visibility(&self) -> DatabaseVerusSyntax {
        let mut syntax = DatabaseVerusSyntax::default();
        for invocation in self
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            if let Some(payload) = self.verus_payload(&invocation) {
                syntax.names.insert(invocation, payload.visible_names());
            }
        }
        syntax
    }
}

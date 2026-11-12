//! Memoized Verus dialect payloads selected from canonical Rust syntax.
use super::{super::database::SemanticDatabase, DatabaseSource};
use crate::{VerusCrateIr, verus_ir::VerusPayloadFacts};
use ra_ap_syntax::{AstNode, ast, ast::HasAttrs};
#[cfg(feature = "visibility")]
mod visibility;
#[cfg(feature = "visibility")]
pub(crate) use visibility::DatabaseVerusSyntax;

#[salsa::interned]
struct Payload<'db> {
    text: String,
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn dialect_facts<'db>(
    db: &'db dyn SemanticDatabase,
    key: Payload<'db>,
) -> Option<VerusPayloadFacts> {
    tracing::debug!(target: "cordial::semantic::queries", function = "payload", query = "verus_dialect", "semantic query executed");
    crate::verus_ir::payload_facts(key.text(db))
}

impl DatabaseSource<'_> {
    /// Only this snapshot can select payloads; consumers receive owned facts.
    /// `is_verus` says which invocations are the Verus macro: the caller knows
    /// what a path resolves to, the snapshot does not.
    #[tracing::instrument(level = "debug", skip(self, is_verus))]
    pub(crate) fn verus_facts(
        &self,
        module_path: &str,
        is_verus: &dyn Fn(&ast::MacroCall) -> bool,
    ) -> VerusCrateIr {
        let mut functions = Vec::new();
        let mut enums = Vec::new();
        for invocation in self
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            let Some(payload) = self.verus_payload(&invocation, is_verus) else {
                continue;
            };
            let cfg_test = invocation
                .syntax()
                .ancestors()
                .filter_map(ast::Module::cast)
                .any(|module| {
                    module.attrs().any(|attr| {
                        self.cfg_condition(&attr)
                            .is_some_and(|(predicate, _)| predicate == "test")
                    })
                });
            let facts = payload
                .facts()
                .in_context(self.path(), module_path, cfg_test);
            functions.extend(facts.functions().clone());
            enums.extend(facts.enums().clone());
        }
        VerusCrateIr::new(functions, enums)
    }

    #[tracing::instrument(level = "debug", skip(self, invocation, is_verus))]
    fn verus_payload(
        &self,
        invocation: &ast::MacroCall,
        is_verus: &dyn Fn(&ast::MacroCall) -> bool,
    ) -> Option<&VerusPayloadFacts> {
        if !is_verus(invocation) {
            return None;
        }
        let tree = invocation.token_tree()?;
        let text = tree.syntax().text().to_string();
        let inner = text.get(1..text.len().saturating_sub(1))?;
        let line = self.line(tree.syntax().text_range().start());
        let key = Payload::new(
            self.database(),
            format!("{}{inner}", "\n".repeat(line.saturating_sub(1) as usize)),
        );
        dialect_facts(self.database(), key).as_ref()
    }
}

#[cfg(feature = "panics")]
/// The `verus!` macro of an embedded program, which is text inside a string
/// literal: no crate scopes its paths, so the only evidence is how it is written.
#[tracing::instrument(level = "trace", skip(invocation))]
pub(crate) fn written_as_verus(invocation: &ast::MacroCall) -> bool {
    invocation
        .path()
        .is_some_and(|path| path.syntax().text() == "verus")
}

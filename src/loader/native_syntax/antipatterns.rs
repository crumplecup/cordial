//! Memoized contract dialect grammar selected from canonical attributes/macros.
use super::{super::database::SemanticDatabase, DatabaseSource};
use crate::etiquettes::antipatterns::ContractClause;
use ra_ap_syntax::{AstNode, ast};
use std::path::PathBuf;

#[salsa::interned]
struct Payload<'db> {
    text: String,
    file: PathBuf,
    modules: Vec<String>,
    context: String,
    kind: String,
    line: u32,
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn contract_facts<'db>(
    db: &'db dyn SemanticDatabase,
    key: Payload<'db>,
) -> Option<Vec<ContractClause>> {
    tracing::debug!(target: "cordial::semantic::queries", function = "payload", query = "antipattern_contract", "semantic query executed");
    let tokens = key.text(db).parse::<proc_macro2::TokenStream>().ok()?;
    crate::etiquettes::antipatterns::antipattern_contract_payload(
        tokens,
        key.file(db),
        key.modules(db),
        key.context(db),
        key.kind(db),
        *key.line(db),
    )
}

impl DatabaseSource<'_> {
    /// Token-tree grammar is evaluated once per source/dialect/context, and
    /// its durable facts are projected into IR before policy is assessed.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn antipattern_contracts(
        &self,
        tree: &ast::TokenTree,
        modules: &[String],
        context: &str,
        kind: &str,
    ) -> Vec<ContractClause> {
        self.antipattern_contracts_at(
            tree,
            modules,
            context,
            kind,
            self.line(tree.syntax().text_range().start()),
        )
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) fn antipattern_contracts_at(
        &self,
        tree: &ast::TokenTree,
        modules: &[String],
        context: &str,
        kind: &str,
        site_line: u32,
    ) -> Vec<ContractClause> {
        // Foreign token trees cannot inject independent parses into evidence.
        if tree.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return Vec::new();
        }
        let text = tree.syntax().text().to_string();
        let Some(inner) = text.get(1..text.len().saturating_sub(1)) else {
            return Vec::new();
        };
        let line = self.line(tree.syntax().text_range().start());
        let key = Payload::new(
            self.database(),
            format!("{}{inner}", "\n".repeat(line.saturating_sub(1) as usize)),
            self.path().to_path_buf(),
            modules.to_vec(),
            context.to_string(),
            kind.to_string(),
            site_line,
        );
        contract_facts(self.database(), key)
            .clone()
            .unwrap_or_default()
    }

    /// A harness gets a source carrier for its canonical memoized grammar
    /// island. Its tree and line coordinates remain inseparable from its text.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn with_antipattern_harness<T>(
        &self,
        invocation: &ast::MacroCall,
        collect: impl FnOnce(&DatabaseSource<'_>) -> T,
    ) -> Option<T> {
        if invocation.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return None;
        }
        let body = invocation
            .token_tree()?
            .syntax()
            .children()
            .filter_map(ast::TokenTree::cast)
            .filter(|tree| tree.l_curly_token().is_some())
            .last()?;
        let text = self.proof_body_text(&body);
        let key = super::macro_syntax::Payload::new(self.database(), text.clone());
        let parsed = super::macro_syntax::program(self.database(), key);
        if !parsed.errors().is_empty() {
            return None;
        }
        let lines = ra_ap_ide_db::line_index::LineIndex::new(&text);
        let source =
            DatabaseSource::new(self.database(), self.path(), &text, parsed.tree(), &lines)
                .locate_macro_body(self, &body)?;
        Some(collect(&source))
    }
}

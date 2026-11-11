//! Memoized cfg grammar inside canonical opaque attribute token trees.
use super::DatabaseSource;
use crate::loader::database::SemanticDatabase;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};

#[salsa::interned]
struct CfgPayload<'db> {
    text: String,
}

#[tracing::instrument(level = "trace", skip(node))]
fn canonical(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|part| part.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[salsa::tracked(returns(ref))]
#[tracing::instrument(level = "trace", skip(db, key))]
#[hotpath::measure]
fn condition<'db>(
    db: &'db dyn SemanticDatabase,
    key: CfgPayload<'db>,
) -> Option<(String, Option<String>)> {
    // The predicate sits in an opaque token tree, so give the parser the one
    // context in which it is a predicate.
    let parse = ra_ap_syntax::SourceFile::parse(
        &format!("#[cfg({})] fn f() {{}}", key.text(db)),
        ra_ap_syntax::Edition::CURRENT,
    );
    if !parse.errors().is_empty() {
        return None;
    }
    let attr = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Attr::cast)?;
    let ast::Meta::CfgMeta(meta) = attr.meta()? else {
        return None;
    };
    predicate_condition(&meta)
}

/// The canonical predicate and, for `not(single)`, the canonical operand.
#[tracing::instrument(level = "trace", skip(meta))]
fn predicate_condition(meta: &ast::CfgMeta) -> Option<(String, Option<String>)> {
    if meta.comma_token().is_some() {
        return None;
    }
    let predicate = meta.cfg_predicate()?;
    if !valid_meta(&predicate) {
        return None;
    }
    let negated = match &predicate {
        ast::CfgPredicate::CfgComposite(list) if list.keyword()?.text() == "not" => {
            let parts: Vec<_> = list.cfg_predicates().collect();
            if let [part] = parts.as_slice() {
                (valid_meta(part)
                    && !list
                        .syntax()
                        .children_with_tokens()
                        .any(|part| part.kind() == SyntaxKind::COMMA))
                .then(|| canonical(part.syntax()))
            } else {
                None
            }
        }
        _ => None,
    };
    Some((canonical(predicate.syntax()), negated))
}

impl DatabaseSource<'_> {
    #[tracing::instrument(level = "trace", skip(self, attr))]
    pub(crate) fn cfg_condition(&self, attr: &ast::Attr) -> Option<(String, Option<String>)> {
        if attr.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return None;
        }
        let meta = attr.meta()?;
        if let ast::Meta::CfgMeta(meta) = &meta {
            return predicate_condition(meta);
        }
        let ast::Meta::TokenTreeMeta(meta) = meta else {
            return None;
        };
        if meta.path()?.syntax().text() != "cfg" {
            return None;
        }
        let tree = meta.token_tree()?;
        let text = self.text(tree.syntax());
        let inner = text.get(1..text.len().checked_sub(1)?)?;
        condition(
            self.database(),
            CfgPayload::new(self.database(), inner.to_string()),
        )
        .clone()
    }
}

#[tracing::instrument(level = "trace", skip(predicate))]
fn valid_meta(predicate: &ast::CfgPredicate) -> bool {
    match predicate {
        ast::CfgPredicate::CfgAtom(atom) => atom.ident_token().is_some(),
        ast::CfgPredicate::CfgComposite(list) => list.keyword().is_some(),
    }
}

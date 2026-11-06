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

#[tracing::instrument(level = "trace", skip(tokens))]
fn canonical_tokens(tokens: proc_macro2::TokenStream) -> String {
    tokens
        .into_iter()
        .map(|token| match token {
            proc_macro2::TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    proc_macro2::Delimiter::Parenthesis => ("(", ")"),
                    proc_macro2::Delimiter::Bracket => ("[", "]"),
                    proc_macro2::Delimiter::Brace => ("{", "}"),
                    proc_macro2::Delimiter::None => ("", ""),
                };
                let inner = canonical_tokens(group.stream());
                [open, inner.as_str(), close]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            token => token.to_string(),
        })
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
    let meta = syn::parse_str::<syn::Meta>(key.text(db)).ok()?;
    let negated = match &meta {
        syn::Meta::List(list) if list.path.is_ident("not") => list
            .parse_args::<syn::Meta>()
            .ok()
            .map(|meta| canonical_tokens(quote::quote!(#meta))),
        _ => None,
    };
    Some((canonical_tokens(quote::quote!(#meta)), negated))
}

impl DatabaseSource<'_> {
    #[tracing::instrument(level = "trace", skip(self, attr))]
    pub(crate) fn cfg_condition(&self, attr: &ast::Attr) -> Option<(String, Option<String>)> {
        if attr.syntax().ancestors().last().as_ref() != Some(self.tree().syntax()) {
            return None;
        }
        let meta = attr.meta()?;
        if let ast::Meta::CfgMeta(meta) = &meta {
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
            return Some((canonical(predicate.syntax()), negated));
        }
        let ast::Meta::TokenTreeMeta(meta) = meta else {
            return None;
        };
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

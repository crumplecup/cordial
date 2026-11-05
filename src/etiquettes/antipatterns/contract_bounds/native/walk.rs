//! Function and harness context for Creusot and Kani contracts.
use super::{ContractClause, kani, name};
use crate::{
    CordialResult,
    loader::{DatabaseMacroSyntax, DatabaseSource},
};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};

struct Visitor<'a, 'db> {
    source: &'a DatabaseSource<'db>,
    modules: &'a [String],
    verifier: &'a str,
    macros: DatabaseMacroSyntax,
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
pub(super) fn collect(
    source: &DatabaseSource<'_>,
    node: &SyntaxNode,
    modules: &[String],
    verifier: &str,
    stack: &[String],
    in_proof: bool,
    clauses: &mut Vec<ContractClause>,
) -> CordialResult<()> {
    let visitor = Visitor {
        source,
        modules,
        verifier,
        macros: source.proof_macro_syntax(),
    };
    visitor.walk(node, stack, in_proof, clauses)
}

impl Visitor<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    fn walk(
        &self,
        node: &SyntaxNode,
        stack: &[String],
        in_proof: bool,
        clauses: &mut Vec<ContractClause>,
    ) -> CordialResult<()> {
        let mut stack = stack.to_vec();
        let mut in_proof = in_proof;
        if let Some(function) = ast::Fn::cast(node.clone()) {
            let owner = function
                .syntax()
                .parent()
                .and_then(|parent| parent.parent());
            let is_impl = owner
                .as_ref()
                .is_some_and(|node| ast::Impl::can_cast(node.kind()));
            let is_trait = owner
                .as_ref()
                .is_some_and(|node| ast::Trait::can_cast(node.kind()));
            if !is_trait && (self.verifier == "creusot" || !is_impl) {
                stack.extend(function.name().map(|name| name.text().to_string()));
                in_proof = function.attrs().any(|attr| {
                    name(attr.path()).as_deref() == Some("proof")
                        && attr.path().is_some_and(|path| {
                            path.syntax()
                                .descendants()
                                .filter_map(ast::NameRef::cast)
                                .any(|name| name.text() == "kani")
                        })
                });
                if self.verifier == "creusot" {
                    for attr in function.attrs() {
                        if let Some(kind) = attr
                            .path()
                            .filter(|path| path.qualifier().is_none())
                            .and_then(|path| name(Some(path)))
                            && matches!(kind.as_str(), "requires" | "ensures")
                            && let Some(tree) = attr.meta().and_then(|meta| {
                                meta.syntax().children().find_map(ast::TokenTree::cast)
                            })
                        {
                            clauses.extend(self.source.antipattern_contracts_at(
                                &tree,
                                self.modules,
                                &stack.join("::"),
                                &kind,
                                self.source.line(attr.syntax().text_range().start()),
                            ));
                        }
                    }
                }
            }
        }
        if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
            if name(invocation.path()).as_deref() == Some("harness") {
                // The contract policy opens item macros, not statement or
                // expression macros inside a function body.
                if !invocation.syntax().parent().is_some_and(|parent| {
                    ast::SourceFile::can_cast(parent.kind())
                        || ast::ItemList::can_cast(parent.kind())
                }) {
                    return Ok(());
                }
                if let Some(result) = self.source.with_antipattern_harness(&invocation, |island| {
                    let items: Vec<_> = island.tree().items().collect();
                    if let [ast::Item::Fn(function)] = items.as_slice() {
                        collect(
                            island,
                            function.syntax(),
                            self.modules,
                            self.verifier,
                            &stack,
                            in_proof,
                            clauses,
                        )
                    } else {
                        Ok(())
                    }
                }) {
                    result?;
                }
            } else if self.verifier == "kani" && in_proof {
                kani::assertion(
                    self.source,
                    &self.macros,
                    &invocation,
                    self.modules,
                    &stack,
                    clauses,
                )?;
            }
            return Ok(());
        }
        if self.verifier == "kani"
            && in_proof
            && let Some(call) = ast::CallExpr::cast(node.clone())
        {
            kani::assumption(self.source, &call, self.modules, &stack, clauses)?;
        }
        for child in node.children() {
            self.walk(&child, &stack, in_proof, clauses)?;
        }
        Ok(())
    }
}

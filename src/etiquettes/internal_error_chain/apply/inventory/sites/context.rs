//! Scope transitions for impl owners, source methods and fallible return contracts.
use super::super::{ErrorFromEvidence, ErrorSourceMethodEvidence, ErrorTryScope};
use super::signatures::{mentions, path, plain_path, return_evidence};
use super::{Collector, Context};
use super::{Kind, signatures};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};
use std::collections::BTreeSet;

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, node, inherited))]
    pub(super) fn context(&mut self, node: &SyntaxNode, inherited: &Context) -> Context {
        let mut context = inherited.clone();
        if ast::Module::can_cast(node.kind()) {
            context.depth += 1;
        }
        if let Some(item) = ast::Impl::cast(node.clone()) {
            context.owner = item
                .self_ty()
                .and_then(|ty| match ty {
                    ast::Type::PathType(ty) => ty.path(),
                    _ => None,
                })
                .and_then(|value| {
                    let fact = path(&value);
                    (!fact.qualified && fact.names.len() == 1).then(|| fact.names[0].clone())
                });
            context.error_trait = item
                .trait_()
                .and_then(|ty| match ty {
                    ast::Type::PathType(ty) => ty.path(),
                    _ => None,
                })
                .map(|ty| path(&ty).names)
                .filter(|names| names.last().is_some_and(|name| name == "Error"));
            if let Some(ast::Type::PathType(ty)) = item.trait_()
                && let Some(trait_path) = ty.path()
            {
                let parts = path(&trait_path).names;
                if parts.last().is_some_and(|name| name == "From") {
                    let input = signatures::single_type_argument(&trait_path)
                        .and_then(|ty| plain_path(&ty));
                    let output = item.self_ty().and_then(|ty| plain_path(&ty));
                    // The trait itself isn't a TypePath in the compatibility AST.
                    let names = trait_path
                        .syntax()
                        .children()
                        .flat_map(|child| mentions(&child))
                        .collect();
                    self.record(
                        node,
                        &context,
                        Kind::From {
                            mentions: names,
                            evidence: ErrorFromEvidence::new(parts, input, output),
                        },
                    );
                }
            }
        }
        if let Some(function) = ast::Fn::cast(node.clone()) {
            let impl_method = node
                .parent()
                .and_then(|list| list.parent())
                .is_some_and(|item| ast::Impl::can_cast(item.kind()));
            let trait_or_foreign = node.parent().is_some_and(|list| {
                ast::ExternItemList::can_cast(list.kind())
                    || list
                        .parent()
                        .is_some_and(|item| ast::Trait::can_cast(item.kind()))
            });
            if impl_method {
                context.source_site = None;
                if function.name().is_some_and(|name| name.text() == "source")
                    && context.owner.is_some()
                    && let Some(trait_path) = &context.error_trait
                {
                    let text = function
                        .ret_type()
                        .and_then(|ty| ty.ty())
                        .map(|ty| self.file.text(ty.syntax()).to_string());
                    context.source_site = Some(self.events.len());
                    self.record(
                        node,
                        &context,
                        Kind::Source(ErrorSourceMethodEvidence::new(
                            trait_path.clone(),
                            text,
                            BTreeSet::new(),
                        )),
                    );
                }
            }
            if !trait_or_foreign {
                let output = function.ret_type().and_then(|ty| ty.ty());
                context.returns = Some(
                    output
                        .as_ref()
                        .map(|ty| mentions(ty.syntax()))
                        .unwrap_or_default(),
                );
                context.evidence = Some(return_evidence(output.as_ref(), ErrorTryScope::Function));
            }
        }
        if let Some(closure) = ast::ClosureExpr::cast(node.clone()) {
            let output = closure.ret_type().and_then(|ty| ty.ty());
            context.returns = output.as_ref().map(|ty| mentions(ty.syntax()));
            context.evidence = Some(return_evidence(
                output.as_ref(),
                if output.is_some() {
                    ErrorTryScope::Closure
                } else {
                    ErrorTryScope::InferredClosure
                },
            ));
        }
        let block_scope = if ast::BlockExpr::cast(node.clone())
            .is_some_and(|block| block.async_token().is_some())
        {
            Some(ErrorTryScope::AsyncBlock)
        } else if ast::BlockExpr::cast(node.clone())
            .is_some_and(|block| block.try_block_modifier().is_some())
        {
            Some(ErrorTryScope::TryBlock)
        } else {
            None
        };
        if let Some(scope) = block_scope {
            context.returns = None;
            context.evidence = Some(return_evidence(None, scope));
        }
        context
    }
}

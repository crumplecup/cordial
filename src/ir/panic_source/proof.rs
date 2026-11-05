//! The existing bare-name Kani contract collected from issued database syntax.
use super::PanicProofGraph;
use super::syntax::{cfg_flag, path_name};
use crate::loader::DatabaseMacroSyntax;
use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasAttrs, ast::HasName};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct ProofCollector {
    calls: BTreeMap<String, Vec<String>>,
    roots: BTreeSet<String>,
    stack: Vec<String>,
}

impl ProofCollector {
    #[tracing::instrument(level = "debug", skip(self, node, macros))]
    pub(super) fn walk(&mut self, node: &SyntaxNode, macros: &DatabaseMacroSyntax, kani: bool) {
        let kani = kani
            || ast::Module::cast(node.clone()).is_some_and(|item| cfg_flag(&item, "kani"))
            || ast::Impl::cast(node.clone()).is_some_and(|item| cfg_flag(&item, "kani"));
        if let Some(function) = ast::Fn::cast(node.clone())
            && super::syntax::tracked_function(&function)
            && let Some(name) = function.name()
        {
            let name = name.text().to_string();
            let kani = kani || cfg_flag(&function, "kani");
            if kani
                || function.attrs().any(|attr| {
                    attr.meta()
                        .and_then(|meta| meta.path())
                        .is_some_and(|path| {
                            path.syntax().text().to_string().replace(' ', "") == "kani::proof"
                        })
                })
            {
                self.roots.insert(name.clone());
            }
            self.calls.entry(name.clone()).or_default();
            self.stack.push(name);
            for child in node.children() {
                self.walk(&child, macros, kani);
            }
            self.stack.pop();
            return;
        }
        let callee = if let Some(call) = ast::CallExpr::cast(node.clone()) {
            match call.expr() {
                Some(ast::Expr::PathExpr(path)) => path.path().and_then(|path| path_name(&path)),
                _ => None,
            }
        } else {
            ast::MethodCallExpr::cast(node.clone())
                .and_then(|call| call.name_ref())
                .map(|name| name.text().to_string())
        };
        if let Some(callee) = callee
            && let Some(caller) = self.stack.last()
        {
            self.calls.entry(caller.clone()).or_default().push(callee);
        }
        if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
            for item in macros.items(&invocation) {
                self.walk(item.syntax(), macros, kani);
            }
            for arg in macros.arguments(&invocation) {
                self.walk(arg.syntax(), macros, kani);
            }
            return;
        }
        for child in node.children() {
            self.walk(&child, macros, kani);
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn finish(mut self) -> PanicProofGraph {
        for calls in self.calls.values_mut() {
            calls.sort();
            calls.dedup();
        }
        PanicProofGraph::new(self.calls, self.roots)
    }
}

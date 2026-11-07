//! Getter receiver evidence from canonical Rust and loader-issued macro islands.
use super::{ConsumerSite, GetterConsumerFacts, ReceiverRecipe, UseKind};
use crate::loader::{DatabaseMacroSyntax, DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasGenericParams, HasName},
};
use std::collections::BTreeMap;
use tracing::instrument;

mod contracts;
mod expressions;
mod macros;

#[derive(Clone)]
struct Scope {
    bindings: BTreeMap<String, ReceiverRecipe>,
    functions: BTreeMap<String, ReceiverRecipe>,
    owner: ReceiverRecipe,
    local_types: bool,
}

struct Collector<'a, 'db> {
    file: &'a DatabaseSource<'db>,
    syntax: DatabaseMacroSyntax,
    scope: Scope,
    facts: Vec<ConsumerSite>,
}

impl GetterConsumerFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GetterConsumerFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut collector = Collector {
            file,
            syntax: file.getter_macro_syntax(),
            facts: Vec::new(),
            scope: Scope {
                bindings: BTreeMap::new(),
                functions: BTreeMap::new(),
                owner: ReceiverRecipe::Unknown,
                local_types: false,
            },
        };
        collector.visit(file.tree().syntax(), 0);
        Self {
            methods: contracts::methods(file),
            uses: collector.facts,
        }
    }
}

impl Collector<'_, '_> {
    #[instrument(level = "trace", skip(self, node))]
    fn visit(&mut self, node: &SyntaxNode, offset: usize) {
        if matches!(node.kind(), SyntaxKind::TOKEN_TREE | SyntaxKind::ATTR) {
            return;
        }
        if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
            self.invocation(&invocation, offset);
            return;
        }
        if let Some(definition) = ast::MacroRules::cast(node.clone()) {
            if let Some(tree) = definition.token_tree() {
                self.opaque(&tree, offset);
            }
            return;
        }
        let saved = self.scope.clone();
        let scoped = if let Some(item) = ast::Module::cast(node.clone()) {
            self.scope.local_types |= item.item_list().is_some();
            true
        } else if let Some(item) = ast::Impl::cast(node.clone()) {
            self.scope.owner = item
                .self_ty()
                .map_or(ReceiverRecipe::Unknown, |ty| self.ty(&ty));
            self.scope.local_types |= type_parameters(&item);
            true
        } else if let Some(function) = ast::Fn::cast(node.clone()).filter(|function| {
            !function
                .syntax()
                .parent()
                .and_then(|parent| parent.parent())
                .is_some_and(|parent| ast::Trait::can_cast(parent.kind()))
        }) {
            if let Some(body) = function.body() {
                let associated = function
                    .syntax()
                    .parent()
                    .and_then(|p| p.parent())
                    .is_some_and(|p| ast::Impl::can_cast(p.kind()));
                if !associated {
                    self.scope.owner = ReceiverRecipe::Unknown;
                }
                self.scope.bindings.clear();
                self.scope.functions.clear();
                self.scope.local_types |= type_parameters(&function);
                if let Some(list) = function.param_list() {
                    if list.self_param().is_some() {
                        self.scope
                            .bindings
                            .insert("self".to_string(), self.scope.owner.clone());
                    }
                    for param in list.params() {
                        if let Some(pat) = param.pat() {
                            let value = param
                                .ty()
                                .map_or(ReceiverRecipe::Unknown, |ty| self.ty(&ty));
                            expressions::bind(&mut self.scope.bindings, &pat, value);
                        }
                    }
                }
                self.visit(body.syntax(), offset);
                self.scope = saved;
                return;
            }
            false
        } else if let Some(block) = ast::StmtList::cast(node.clone()) {
            self.scope.local_types |= block.statements().any(|stmt| match stmt {
                ast::Stmt::Item(item) => matches!(
                    item,
                    ast::Item::Use(_)
                        | ast::Item::TypeAlias(_)
                        | ast::Item::Struct(_)
                        | ast::Item::Enum(_)
                        | ast::Item::Module(_)
                        | ast::Item::MacroRules(_)
                ),
                _ => false,
            });
            for stmt in block.statements() {
                if let ast::Stmt::Item(ast::Item::Fn(function)) = stmt
                    && let Some(name) = function.name()
                {
                    let value = if type_parameters(&function) {
                        ReceiverRecipe::Unknown
                    } else {
                        function
                            .ret_type()
                            .and_then(|ret| ret.ty())
                            .map_or(ReceiverRecipe::Unknown, |ty| self.ty(&ty))
                    };
                    self.scope
                        .functions
                        .entry(name.text().to_string())
                        .and_modify(|v| *v = ReceiverRecipe::Unknown)
                        .or_insert(value);
                }
            }
            true
        } else if let Some(closure) = ast::ClosureExpr::cast(node.clone()) {
            if let Some(list) = closure.param_list() {
                for param in list.params() {
                    if let Some(pat) = param.pat() {
                        expressions::bind(&mut self.scope.bindings, &pat, ReceiverRecipe::Unknown);
                    }
                }
            }
            true
        } else {
            false
        };
        if let Some(local) = ast::LetStmt::cast(node.clone()) {
            let value = if let Some(ty) = local.ty() {
                self.ty(&ty)
            } else {
                local
                    .initializer()
                    .map_or(ReceiverRecipe::Unknown, |expr| self.receiver(&expr))
            };
            for child in node.children() {
                self.visit(&child, offset);
            }
            if let Some(pat) = local.pat() {
                expressions::bind(&mut self.scope.bindings, &pat, value);
            }
            return;
        }
        if let Some(pat) = ast::IdentPat::cast(node.clone())
            && let Some(name) = pat.name()
        {
            self.scope
                .bindings
                .insert(name.text().to_string(), ReceiverRecipe::Unknown);
        }
        self.expression(node, offset);
        for child in node.children() {
            self.visit(&child, offset);
        }
        if scoped {
            self.scope = saved;
        }
    }

    #[instrument(level = "trace", skip(self, receiver, kind))]
    fn record(&mut self, start: usize, end: usize, receiver: ReceiverRecipe, kind: UseKind) {
        let (line, column) = position(self.file.source(), start);
        let (end_line, end_column) = position(self.file.source(), end);
        self.facts.push(ConsumerSite {
            line,
            column,
            receiver,
            kind,
            end_line,
            end_column,
        });
    }
}

#[instrument(level = "trace", skip(item))]
fn type_parameters(item: &impl HasGenericParams) -> bool {
    item.generic_param_list().is_some_and(|list| {
        list.generic_params()
            .any(|param| matches!(param, ast::GenericParam::TypeParam(_)))
    })
}

#[instrument(level = "trace", skip(source))]
fn position(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
    let start = prefix.rfind('\n').map_or(0, |index| index + 1);
    (line, prefix[start..].chars().count())
}

//! Conversion contracts from the loader's canonical rust-analyzer tree.
use super::super::{
    ErrorEdgeEvidence, ErrorEdgeField, ErrorFromEvidence, ErrorSourceMethodEvidence,
    ErrorTryEvidence, ErrorTryScope,
};
use super::{CalleeGuard, ConversionFileFacts, Event, Kind, PathFact};
use crate::etiquettes::internal_error_chain::apply::graph_apply::GraphExprFacts;
use crate::loader::{DatabaseMacroSyntax, DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasName},
};
use std::collections::BTreeSet;
mod attributes;
mod context;
mod signatures;
use signatures::{mentions, path, plain_path, return_evidence};

#[derive(Clone, Default)]
struct Context {
    depth: usize,
    owner: Option<String>,
    error_trait: Option<Vec<String>>,
    source_site: Option<usize>,
    returns: Option<BTreeSet<String>>,
    evidence: Option<ErrorTryEvidence>,
    guards: Vec<CalleeGuard>,
}
struct Collector<'a, 'db> {
    file: &'a DatabaseSource<'db>,
    macros: DatabaseMacroSyntax,
    events: Vec<Event>,
}
impl ConversionFileFacts {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ConversionFileFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut collector = Collector {
            file,
            macros: file.error_apply_macro_syntax(),
            events: Vec::new(),
        };
        collector.node(
            file.tree().syntax(),
            &Context {
                returns: Some(BTreeSet::new()),
                ..Context::default()
            },
        );
        Self::new(collector.events)
    }
}
impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, node, context, kind))]
    fn record(&mut self, node: &SyntaxNode, context: &Context, kind: Kind) {
        let range = node.text_range();
        let start = node
            .descendants_with_tokens()
            .filter_map(|part| part.into_token())
            .find(|token| {
                !token.kind().is_trivia()
                    || matches!(
                        token.kind(),
                        SyntaxKind::INNER_DOC_COMMENT | SyntaxKind::OUTER_DOC_COMMENT
                    )
            })
            .map_or(range.start(), |token| token.text_range().start());
        let end = node
            .descendants_with_tokens()
            .filter_map(|part| part.into_token())
            .filter(|token| !token.kind().is_trivia())
            .last()
            .map_or(range.end(), |token| token.text_range().end());
        let (start, end) = if matches!(kind, Kind::Macro { .. }) {
            let start = ast::MacroCall::cast(node.clone())
                .and_then(|invocation| invocation.path())
                .map(|path| path.syntax().text_range().start())
                .or_else(|| {
                    ast::MacroRules::cast(node.clone())
                        .and_then(|item| item.macro_rules_token())
                        .map(|token| token.text_range().start())
                })
                .unwrap_or(start);
            let end = node
                .children()
                .filter_map(ast::TokenTree::cast)
                .last()
                .map_or(end, |tree| tree.syntax().text_range().end());
            (start, end)
        } else {
            (start, end)
        };
        let position = |offset| {
            // Columns count chars, matching `proc_macro2` spans; rust-analyzer's are UTF-8 bytes.
            let index = self.file.line_index();
            let value = index.line_col(offset);
            let column = index
                .to_wide(ra_ap_ide_db::line_index::WideEncoding::Utf32, value)
                .map_or(value.col, |wide| wide.col);
            (value.line as usize + 1, column as usize)
        };
        self.events.push(Event::new(
            position(start),
            position(end),
            context.depth,
            context.owner.clone(),
            context.guards.clone(),
            kind,
        ));
    }
    #[tracing::instrument(level = "trace", skip(self, node, inherited))]
    fn node(&mut self, node: &SyntaxNode, inherited: &Context) {
        if matches!(
            node.kind(),
            SyntaxKind::TOKEN_TREE
                | SyntaxKind::CFG_META
                | SyntaxKind::CFG_ATTR_META
                | SyntaxKind::UNSAFE_META
        ) {
            return;
        }
        let context = self.context(node, inherited);
        self.site(node, &context);
        if let Some(call) = ast::CallExpr::cast(node.clone())
            && let Some(ast::Expr::PathExpr(callee)) = call.expr()
            && let Some(callee_path) = callee.path()
        {
            let fact = path(&callee_path);
            self.record(node, &context, Kind::Caller(fact.clone()));
            let mut guarded = context.clone();
            guarded
                .guards
                .push(CalleeGuard::new(fact, context.owner.clone()));
            self.node(callee.syntax(), &guarded);
            if let Some(args) = call.arg_list() {
                for arg in args.args() {
                    self.node(arg.syntax(), &context);
                }
            }
            return;
        }
        self.children(node, &context);
    }
    #[tracing::instrument(level = "trace", skip(self, node, context))]
    fn children(&mut self, node: &SyntaxNode, context: &Context) {
        let children: Vec<_> = node.children().collect();
        if let Some(where_clause) = children
            .iter()
            .find(|child| ast::WhereClause::can_cast(child.kind()))
        {
            // The compatibility traversal visits the complete generics contract
            // before parameters, return types, impl headers and tuple fields.
            let header = |child: &&SyntaxNode| {
                matches!(
                    child.kind(),
                    SyntaxKind::ATTR
                        | SyntaxKind::NAME
                        | SyntaxKind::VISIBILITY
                        | SyntaxKind::GENERIC_PARAM_LIST
                )
            };
            for child in children.iter().filter(header) {
                self.node(child, context);
            }
            self.node(where_clause, context);
            for child in children
                .iter()
                .filter(|child| !header(child) && *child != where_clause)
            {
                self.node(child, context);
            }
        } else {
            for child in children {
                self.node(&child, context);
            }
        }
    }
    #[tracing::instrument(level = "trace", skip(self, node, context))]
    fn site(&mut self, node: &SyntaxNode, context: &Context) {
        if let Some(record) = ast::RecordExpr::cast(node.clone())
            && let Some(path_node) = record.path()
        {
            let rest = record
                .record_expr_field_list()
                .is_some_and(|fields| fields.spread().is_some());
            self.record(
                node,
                context,
                Kind::Record {
                    path: path(&path_node),
                    rest,
                },
            );
        }
        if let Some(expression) = ast::PathExpr::cast(node.clone())
            && let Some(path_node) = expression.path()
        {
            self.record(node, context, Kind::Associated(path(&path_node)));
        }
        if let Some(field) = ast::FieldExpr::cast(node.clone())
            && let Some(name) = field.name_ref()
        {
            let name = name.text().to_string();
            let self_base = field.expr().is_some_and(|expr| matches!(expr, ast::Expr::PathExpr(expr) if expr.path().is_some_and(|value| { let fact = path(&value); fact.bare && fact.names == ["self"] })));
            if self_base
                && let Some(index) = context.source_site
                && let Kind::Source(evidence) = &mut self.events[index].kind
            {
                evidence.record_field_name(name.clone());
            }
            self.record(node, context, Kind::Field(name));
        }
        if let Some(pattern) = ast::RecordPat::cast(node.clone())
            && let Some(path_node) = pattern.path()
        {
            let fields = pattern
                .record_pat_field_list()
                .map(|list| {
                    list.fields()
                        .filter_map(|field| {
                            field
                                .name_ref()
                                .map(|name| name.text().to_string())
                                .or_else(|| {
                                    field.pat().and_then(|pat| match pat {
                                        ast::Pat::IdentPat(pat) => {
                                            pat.name().map(|name| name.text().to_string())
                                        }
                                        _ => None,
                                    })
                                })
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.record(
                node,
                context,
                Kind::Pattern {
                    path: path(&path_node),
                    fields,
                },
            );
        }
        if ast::TryExpr::can_cast(node.kind()) {
            self.record(
                node,
                context,
                Kind::Try {
                    returns: context.returns.clone(),
                    evidence: context.evidence.clone(),
                },
            );
        }
        if let Some(ty) = ast::PathType::cast(node.clone())
            && signatures::is_type_path(node)
            && let Some(path_node) = ty.path()
        {
            self.record(node, context, Kind::Type(path(&path_node)));
        }
        if let Some(alias) = ast::TypeAlias::cast(node.clone()).filter(|_| {
            !node
                .parent()
                .is_some_and(|parent| ast::AssocItemList::can_cast(parent.kind()))
        }) {
            let mut names = alias
                .ty()
                .map(|ty| mentions(ty.syntax()))
                .unwrap_or_default();
            if let Some(name) = alias.name() {
                names.insert(name.text().to_string());
            }
            self.record(
                node,
                context,
                Kind::Alias {
                    names,
                    stable_error: signatures::stable_alias(self.file, &alias),
                },
            );
        }
        if let Some(item) = ast::Use::cast(node.clone()) {
            let mut names = BTreeSet::new();
            let mut glob = false;
            if let Some(tree) = item.use_tree() {
                signatures::imports(&tree, &mut names, &mut glob);
            }
            self.record(node, context, Kind::Import { names, glob });
        }
        if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
            let (constructs, fields, unhandled) = GraphExprFacts::conversion_macro(
                invocation.token_tree(),
                Some(&invocation),
                &self.macros,
            );
            self.record(
                node,
                context,
                Kind::Macro {
                    constructs,
                    fields,
                    unhandled,
                },
            );
        }
        if let Some(item) = ast::MacroRules::cast(node.clone()) {
            let (constructs, fields, unhandled) =
                GraphExprFacts::conversion_macro(item.token_tree(), None, &self.macros);
            self.record(
                node,
                context,
                Kind::Macro {
                    constructs,
                    fields,
                    unhandled,
                },
            );
        }
        if let Some(item) = ast::Struct::cast(node.clone()) {
            self.structure(&item, context);
        }
    }
    #[tracing::instrument(level = "trace", skip(self, item, context))]
    fn structure(&mut self, item: &ast::Struct, context: &Context) {
        use ast::HasAttrs;
        let Some(name) = item.name().map(|name| name.text().to_string()) else {
            return;
        };
        for attr in item.attrs() {
            if attributes::blocking(&attr) {
                self.record(
                    attr.syntax(),
                    context,
                    Kind::Attribute {
                        name: name.clone(),
                        stable_file: attributes::stable_file(self.file, item, &attr),
                    },
                );
            }
        }
        if let Some(ast::FieldList::RecordFieldList(list)) = item.field_list() {
            for field in list.fields() {
                let edge_field = field.name().and_then(|name| match name.text() {
                    "source" => Some(ErrorEdgeField::Source),
                    "kind" => Some(ErrorEdgeField::Kind),
                    _ => None,
                });
                if let Some(edge_field) = edge_field {
                    let evidence = field.ty().and_then(|ty| {
                        ErrorEdgeEvidence::collect_native(self.file, &ty, edge_field)
                    });
                    self.record(
                        field.syntax(),
                        context,
                        Kind::Edge {
                            name: name.clone(),
                            evidence,
                        },
                    );
                }
            }
        }
    }
}

//! Which conversion event, if any, a syntax node contributes.
use super::super::super::{ErrorEdgeEvidence, ErrorEdgeField};
use super::super::signatures::{mentions, path};
use super::super::{Kind, attributes, signatures};
use super::{Collector, Context};
use crate::etiquettes::internal_error_chain::apply::graph_apply::GraphExprFacts;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};
use std::collections::BTreeSet;

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, node, context))]
    pub(super) fn site(&mut self, node: &SyntaxNode, context: &Context) {
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

//! Conversion contracts from the loader's canonical rust-analyzer tree.
use super::super::ErrorTryEvidence;
use super::signatures::path;
use super::{CalleeGuard, ConversionFileFacts, Event, Kind};
use crate::loader::{DatabaseMacroSyntax, DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList},
};
use std::collections::BTreeSet;

mod context;
mod recognize;

/// Scope inherited by nested syntax during one walk of a file.
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
}

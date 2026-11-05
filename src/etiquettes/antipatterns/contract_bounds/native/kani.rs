//! Kani clause shapes use the loader's expression nodes without reparsing.
use super::super::{
    facts::ContractCall,
    index::tokens::{canonicalize_type_text, strip_turbofish},
};
use super::{ContractClause, is_trivial, make_finding, name, site_context};
use crate::{
    CordialResult,
    loader::{
        DatabaseMacroSyntax, DatabaseSource, database_token_text as token_text,
        database_token_text_excluding as token_text_excluding,
    },
};
use ra_ap_syntax::{AstNode, ast, ast::HasArgList};

#[derive(derive_new::new)]
struct Clause {
    text: String,
    call: ContractCall,
    canonical_body: String,
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
pub(super) fn assertion(
    source: &DatabaseSource<'_>,
    syntax: &DatabaseMacroSyntax,
    invocation: &ast::MacroCall,
    modules: &[String],
    stack: &[String],
    clauses: &mut Vec<ContractClause>,
) -> CordialResult<()> {
    let Some(path) = invocation.path().filter(|path| path.qualifier().is_none()) else {
        return Ok(());
    };
    let name = name(Some(path));
    if !matches!(name.as_deref(), Some("assert" | "assert_eq")) {
        return Ok(());
    }
    let args = syntax.arguments(invocation);
    let Some(first) = args.first() else {
        return Ok(());
    };
    let clause = if name.as_deref() == Some("assert") {
        if args.len() > 2 {
            return Ok(());
        }
        Clause::new(
            token_text(first.syntax()),
            call_shape(first),
            canonical_body(first),
        )
    } else {
        let Some(second) = args.get(1) else {
            return Ok(());
        };
        Clause::new(
            format!(
                "{} == {}",
                token_text(first.syntax()),
                token_text(second.syntax())
            ),
            ContractCall::None,
            format!("{}=={}", canonical_body(first), canonical_body(second)),
        )
    };
    let Some(tree) = invocation.token_tree() else {
        return Ok(());
    };
    let root = first
        .syntax()
        .ancestors()
        .last()
        .unwrap_or_else(|| first.syntax().clone());
    let prefix = root.text().to_string();
    let offset = usize::from(first.syntax().text_range().start() - root.text_range().start());
    let line = source.line(tree.syntax().text_range().start())
        + prefix[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u32;
    record(
        source,
        site_context(modules, &stack.join("::")),
        "ensures",
        clause,
        line,
        clauses,
    )
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
pub(super) fn assumption(
    source: &DatabaseSource<'_>,
    call: &ast::CallExpr,
    modules: &[String],
    stack: &[String],
    clauses: &mut Vec<ContractClause>,
) -> CordialResult<()> {
    let Some(ast::Expr::PathExpr(function)) = call.expr() else {
        return Ok(());
    };
    if name(function.path()).as_deref() != Some("assume")
        || !function.path().is_some_and(|path| {
            path.syntax()
                .descendants()
                .filter_map(ast::NameRef::cast)
                .any(|name| name.text() == "kani")
        })
    {
        return Ok(());
    }
    let Some(first) = call.arg_list().and_then(|list| list.args().next()) else {
        return Ok(());
    };
    record(
        source,
        site_context(modules, &stack.join("::")),
        "requires",
        Clause::new(
            token_text(first.syntax()),
            call_shape(&first),
            canonical_body(&first),
        ),
        source.line(first.syntax().text_range().start()),
        clauses,
    )
}

#[tracing::instrument(level = "trace", skip(expr))]
fn call_shape(expr: &ast::Expr) -> ContractCall {
    let call = match expr {
        ast::Expr::CallExpr(call) => Some(call.clone()),
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Not) => {
            prefix.expr().and_then(|expr| match expr {
                ast::Expr::CallExpr(call) => Some(call),
                _ => None,
            })
        }
        _ => None,
    };
    let Some(ast::Expr::PathExpr(function)) = call.and_then(|call| call.expr()) else {
        return ContractCall::None;
    };
    let Some(path) = function.path() else {
        return ContractCall::None;
    };
    let Some(predicate) = name(Some(path.clone())) else {
        return ContractCall::None;
    };
    let mut first = path.clone();
    while let Some(qualifier) = first.qualifier() {
        first = qualifier;
    }
    if let Some(ty) = first
        .segment()
        .and_then(|segment| segment.type_anchor())
        .and_then(|anchor| anchor.ty())
    {
        return ContractCall::Typed {
            predicate,
            prefix: canonicalize_type_text(&token_text(ty.syntax())),
        };
    }
    match path.qualifier() {
        None => ContractCall::Bare(predicate),
        Some(prefix) => ContractCall::Typed {
            predicate,
            prefix: canonicalize_type_text(&strip_turbofish(&token_text(prefix.syntax()))),
        },
    }
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn record(
    source: &DatabaseSource<'_>,
    context: String,
    kind: &str,
    clause: Clause,
    line: u32,
    clauses: &mut Vec<ContractClause>,
) -> CordialResult<()> {
    if !is_trivial(&clause.text) {
        clauses.push(ContractClause::new(
            make_finding(context, source.path(), line, &clause.text)?,
            "kani".into(),
            kind.into(),
            clause.call,
            clause.canonical_body,
            Vec::new(),
        ));
    }
    Ok(())
}

#[tracing::instrument(level = "trace", skip(expr))]
fn canonical_body(expr: &ast::Expr) -> String {
    let excluded: Vec<_> = expr
        .syntax()
        .children()
        .filter_map(ast::Attr::cast)
        .filter(|attr| attr.as_simple_atom().as_deref() == Some("trigger"))
        .map(|attr| attr.syntax().text_range())
        .collect();
    canonicalize_type_text(&token_text_excluding(expr.syntax(), &excluded))
}

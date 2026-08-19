//! Internal error-chain compliance layer: typed foreign errors discarded or
//! stringified at internal error boundaries.
//!
//! Gated as a whole unit by `#[cfg(feature = "internal_error_chain")]` on
//! the `mod compliance_layer;` declaration in `error_ir/mod.rs` — nothing
//! inside this file needs its own `#[cfg]`.

use syn::spanned::Spanned;
use syn::{Expr, ExprCall, ExprIf, ExprMatch, ExprPath, Stmt};

use super::visitor::{SiteCtx, pat_is_err, raw_expr_snippet, truncate_snippet};
use crate::etiquettes::error_sites::infer_foreign_error_type;
use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

use tracing::instrument;
/// Per-file accumulator for the `internal_error_chain` compliance layer.
#[derive(Default)]
pub(super) struct ComplianceLayer {
    findings: Vec<InternalErrorComplianceFinding>,
}

impl ComplianceLayer {
    #[instrument(level = "debug")]
    pub(super) fn new() -> Self {
        Self::default()
    }

    #[instrument(level = "debug", skip(self, rule_id, ctx))]
    fn push(
        &mut self,
        rule_id: InternalErrorComplianceId,
        line: u32,
        snippet: String,
        foreign_error_type: Option<String>,
        internal_constructor: Option<String>,
        ctx: &SiteCtx,
    ) {
        self.findings.push(InternalErrorComplianceFinding {
            crate_name: ctx.crate_name.clone(),
            rule_id,
            context: ctx.context.clone(),
            file: ctx.file.clone(),
            line,
            snippet,
            foreign_error_type,
            internal_constructor,
        });
    }

    #[instrument(level = "debug", skip(self, receiver, converter, ctx))]
    pub(super) fn on_map_err(
        &mut self,
        receiver: &Expr,
        converter: &Expr,
        line: u32,
        ctx: &SiteCtx,
    ) {
        let source_snippet = compliance_expr_snippet(receiver);
        let foreign_type = infer_foreign_error_type(&source_snippet).map(|(ty, _, _)| ty);
        if foreign_type.is_none() {
            return;
        }

        if let Some(constructor) = internal_leaf_constructor(converter) {
            self.push(
                InternalErrorComplianceId::DiscardTyped001,
                line,
                format!("{source_snippet}.map_err(…)"),
                foreign_type,
                Some(constructor),
                ctx,
            );
            return;
        }

        if compliance_map_err_stringifies(converter) {
            self.push(
                InternalErrorComplianceId::StringifyForeign001,
                line,
                format!("{source_snippet}.map_err(…)"),
                foreign_type,
                None,
                ctx,
            );
        }
    }

    #[instrument(level = "debug", skip(self, payload, source_expr, ctx))]
    fn on_err_payload(
        &mut self,
        payload: &Expr,
        source_expr: Option<&Expr>,
        line: u32,
        site: &str,
        ctx: &SiteCtx,
    ) {
        let foreign_type = source_expr
            .and_then(|expr| {
                infer_foreign_error_type(&compliance_expr_snippet(expr)).map(|(ty, _, _)| ty)
            })
            .or_else(|| {
                infer_foreign_error_type(&compliance_expr_snippet(payload)).map(|(ty, _, _)| ty)
            })
            .or_else(|| foreign_binding_in_expr(payload));
        if foreign_type.is_none() {
            return;
        }
        if let Some(constructor) = internal_leaf_constructor(payload) {
            self.push(
                InternalErrorComplianceId::DiscardTyped001,
                line,
                site.to_string(),
                foreign_type,
                Some(constructor),
                ctx,
            );
        } else if compliance_expr_contains_to_string(payload) {
            self.push(
                InternalErrorComplianceId::StringifyForeign001,
                line,
                site.to_string(),
                foreign_type,
                None,
                ctx,
            );
        }
    }

    #[instrument(level = "debug", skip(self, expr, ctx))]
    pub(super) fn on_return_err(&mut self, expr: &Expr, line: u32, ctx: &SiteCtx) {
        if let Some(payload) = compliance_err_payload(expr) {
            self.on_err_payload(payload, None, line, "return Err(…)", ctx);
        }
    }

    #[instrument(level = "debug", skip(self, node, ctx))]
    pub(super) fn on_if_let_err(&mut self, node: &ExprIf, ctx: &SiteCtx) {
        let Some(source) = if_let_err_source(&node.cond) else {
            return;
        };
        let Some(payload) = block_err_payload(&node.then_branch) else {
            return;
        };
        self.on_err_payload(
            payload,
            Some(source),
            node.cond.span().start().line as u32,
            "if let Err(…) = …",
            ctx,
        );
    }

    #[instrument(level = "debug", skip(self, node, ctx))]
    pub(super) fn on_match_err(&mut self, node: &ExprMatch, ctx: &SiteCtx) {
        for arm in &node.arms {
            if pat_is_err(&arm.pat)
                && let Some(payload) = compliance_err_payload(&arm.body)
            {
                self.on_err_payload(
                    payload,
                    Some(&node.expr),
                    arm.span().start().line as u32,
                    "match … {{ Err(…) => … }}",
                    ctx,
                );
            }
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_findings(self) -> Vec<InternalErrorComplianceFinding> {
        self.findings
    }
}

#[instrument(level = "debug", skip(expr))]
fn compliance_expr_snippet(expr: &Expr) -> String {
    truncate_snippet(&raw_expr_snippet(expr), 96)
}

#[instrument(level = "debug", skip(expr))]
fn if_let_err_source(expr: &Expr) -> Option<&Expr> {
    let Expr::Let(let_expr) = expr else {
        return None;
    };
    if !pat_is_err(&let_expr.pat) {
        return None;
    }
    Some(&let_expr.expr)
}

#[instrument(level = "debug", skip(expr))]
fn internal_leaf_constructor(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(call) => constructor_from_call(call),
        Expr::Closure(closure) => internal_leaf_constructor(&closure.body),
        Expr::Block(block) => block
            .block
            .stmts
            .iter()
            .find_map(|stmt| stmt_expr(stmt).and_then(internal_leaf_constructor)),
        Expr::Paren(paren) => internal_leaf_constructor(&paren.expr),
        Expr::Group(group) => internal_leaf_constructor(&group.expr),
        _ => None,
    }
}

#[instrument(level = "debug", skip(call))]
fn constructor_from_call(call: &ExprCall) -> Option<String> {
    let path = match &*call.func {
        Expr::Path(path) => path,
        _ => return None,
    };
    let label = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    if is_internal_leaf_constructor(&label) {
        Some(label)
    } else {
        None
    }
}

#[instrument(level = "trace", ret)]
fn is_internal_leaf_constructor(label: &str) -> bool {
    // Constructors that drop or stringify the foreign error. `from` / `syn_parse`
    // / `json_parse` / `cargo_metadata` / `Io(...)` keep the typed source.
    let last = label.rsplit("::").next().unwrap_or(label);
    matches!(last, "invariant")
}

#[instrument(level = "debug", skip(expr))]
fn compliance_map_err_stringifies(expr: &Expr) -> bool {
    match expr {
        Expr::Closure(closure) => compliance_expr_contains_to_string(&closure.body),
        _ => compliance_expr_contains_to_string(expr),
    }
}

#[instrument(level = "debug", skip(expr))]
fn compliance_expr_contains_to_string(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(call) if call.method == "to_string" => {
            expr_uses_error_binding(&call.receiver)
        }
        Expr::Call(call) => {
            compliance_expr_contains_to_string(&call.func)
                || call.args.iter().any(compliance_expr_contains_to_string)
        }
        Expr::MethodCall(call) => {
            compliance_expr_contains_to_string(&call.receiver)
                || call.args.iter().any(compliance_expr_contains_to_string)
        }
        Expr::Closure(closure) => compliance_expr_contains_to_string(&closure.body),
        Expr::Block(block) => block
            .block
            .stmts
            .iter()
            .any(|stmt| stmt_expr(stmt).is_some_and(compliance_expr_contains_to_string)),
        Expr::Macro(mac) => {
            let name = mac
                .mac
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string());
            let interpolates_error =
                matches!(
                    name.as_deref(),
                    Some("format") | Some("format_args") | Some("write") | Some("writeln")
                ) && macro_interpolates_error_binding(&mac.mac.tokens.to_string());
            interpolates_error || mac.mac.tokens.to_string().contains("to_string")
        }
        Expr::Struct(item) => item
            .fields
            .iter()
            .any(|field| compliance_expr_contains_to_string(&field.expr)),
        Expr::Field(field) => compliance_expr_contains_to_string(&field.base),
        Expr::Paren(paren) => compliance_expr_contains_to_string(&paren.expr),
        Expr::Group(group) => compliance_expr_contains_to_string(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr))]
fn foreign_binding_in_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(call) if call.args.iter().any(expr_uses_error_binding) => {
            Some("foreign-bound".to_string())
        }
        Expr::Macro(mac) if mac.mac.tokens.to_string().contains("{e}") => {
            Some("foreign-bound".to_string())
        }
        _ => None,
    }
}

#[instrument(level = "debug", skip(stmt))]
fn stmt_expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

#[instrument(level = "debug")]
fn macro_interpolates_error_binding(tokens: &str) -> bool {
    let compact = tokens.replace(' ', "");
    compact.contains("{e}")
        || compact.contains("{err}")
        || compact.contains("{error}")
        || compact.contains("{e:")
        || compact.contains("{err:")
        || compact.contains("{error:")
        || compact.contains(",e}")
        || compact.contains(",err}")
        || compact.contains(",error}")
        || compact.contains("(e)")
        || compact.contains("(err)")
        || compact.contains("(error)")
}

#[instrument(level = "debug", skip(expr))]
fn expr_uses_error_binding(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| ident == "e" || ident == "err" || ident == "error"),
        Expr::MethodCall(call) if call.method == "to_string" => {
            expr_uses_error_binding(&call.receiver)
        }
        Expr::Macro(mac) => {
            let tokens = mac.mac.tokens.to_string();
            tokens.contains("{e}")
                || tokens.contains("{err}")
                || tokens.contains("{error}")
                || tokens.contains("to_string")
        }
        Expr::Call(call) => call.args.iter().any(expr_uses_error_binding),
        _ => false,
    }
}

#[instrument(level = "debug", skip(block))]
fn block_err_payload(block: &syn::Block) -> Option<&Expr> {
    for stmt in &block.stmts {
        let Stmt::Expr(expr, _) = stmt else {
            continue;
        };
        if let Some(payload) = compliance_err_payload(expr) {
            return Some(payload);
        }
    }
    None
}

#[instrument(level = "debug", skip(expr))]
fn compliance_err_payload(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Return(ret) => ret.expr.as_deref().and_then(compliance_err_payload),
        Expr::Call(call) => match &*call.func {
            Expr::Path(path) if path_is_err(path) => call.args.first(),
            _ => None,
        },
        _ => None,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_err(path: &ExprPath) -> bool {
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Err")
}

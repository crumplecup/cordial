//! Preserve the compliance grammar over canonical database expressions.

use super::{ErrorComplianceFile, SourceErrorCompliance, bindings, expressions as shape};
use crate::etiquettes::{
    error_sites::infer_foreign_error_type,
    internal_error_chain::{InternalErrorComplianceFinding, InternalErrorComplianceId},
};
use crate::ir::error_sites_source::native::{context, excluded_module, snippet, truncate};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{
    AstNode, SyntaxNode, WalkEvent,
    ast::{self, HasArgList},
};
use std::path::Path;

struct Fact {
    node: SyntaxNode,
    rule: InternalErrorComplianceId,
    snippet: String,
    foreign: Option<String>,
    constructor: Option<String>,
}

impl SourceErrorCompliance {
    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceErrorCompliance")]
    pub(crate) fn native_file(
        file: &DatabaseSource<'_>,
        tree: &Path,
        crate_name: &str,
        proc_macro: bool,
    ) -> CordialResult<ErrorComplianceFile> {
        let mut findings = Vec::new();
        let prefix = module_path_from_src_file(tree, file.path());
        let mut walk = file.tree().syntax().preorder();
        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };
            if excluded_module(&node) {
                walk.skip_subtree();
                continue;
            }
            if let Some(expr) = ast::Expr::cast(node) {
                for fact in collect(&expr, proc_macro) {
                    findings.push(
                        InternalErrorComplianceFinding::builder()
                            .crate_name(crate_name.to_owned())
                            .rule_id(fact.rule)
                            .context(context(&fact.node, &prefix))
                            .file(file.path().to_path_buf())
                            .line(file.line(fact.node.text_range().start()))
                            .snippet(fact.snippet)
                            .foreign_error_type(fact.foreign)
                            .internal_constructor(fact.constructor)
                            .build()?,
                    );
                }
            }
        }
        Ok(ErrorComplianceFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            findings,
        })
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
fn foreign(expr: &ast::Expr) -> Option<String> {
    infer_foreign_error_type(&truncate(&snippet(expr), 96)).map(|(ty, _, _)| ty)
}

#[tracing::instrument(level = "trace", skip(expr))]
fn collect(expr: &ast::Expr, proc_macro: bool) -> Vec<Fact> {
    let mut facts = Vec::new();
    match expr {
        ast::Expr::MethodCallExpr(call)
            if call.name_ref().is_some_and(|name| name.text() == "map_err") =>
        {
            if let Some((source, converter)) = call
                .receiver()
                .zip(call.arg_list().and_then(|args| args.args().next()))
            {
                let text = format!("{}.map_err(…)", truncate(&snippet(&source), 96));
                facts.extend(assess(
                    &converter,
                    foreign(&source),
                    bindings::map_err_discards(&converter),
                    expr.syntax(),
                    text,
                    proc_macro,
                ));
            }
        }
        ast::Expr::ReturnExpr(ret) => {
            if let Some(payload) = ret.expr().as_ref().and_then(shape::err_payload) {
                facts.extend(payload_fact(
                    &payload,
                    None,
                    None,
                    expr.syntax(),
                    "return Err(…)",
                    proc_macro,
                ));
            }
        }
        ast::Expr::IfExpr(branch) => {
            if let Some(ast::Expr::LetExpr(condition)) = branch.condition()
                && let Some(binding) = condition.pat().as_ref().and_then(shape::err_binding)
                && let Some(payload) = branch
                    .then_branch()
                    .as_ref()
                    .map(shape::block_exprs)
                    .and_then(|expressions| expressions.iter().find_map(shape::err_payload))
            {
                facts.extend(payload_fact(
                    &payload,
                    condition.expr().as_ref(),
                    binding.as_deref(),
                    condition.syntax(),
                    "if let Err(…) = …",
                    proc_macro,
                ));
            }
        }
        ast::Expr::MatchExpr(expr) => {
            if let Some(arms) = expr.match_arm_list() {
                for arm in arms.arms() {
                    if let Some(binding) = arm.pat().as_ref().and_then(shape::err_binding)
                        && let Some(payload) = arm.expr().as_ref().and_then(shape::err_payload)
                    {
                        facts.extend(payload_fact(
                            &payload,
                            expr.expr().as_ref(),
                            binding.as_deref(),
                            arm.syntax(),
                            "match … {{ Err(…) => … }}",
                            proc_macro,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    facts
}

#[tracing::instrument(level = "trace", skip(payload, source, node))]
fn payload_fact(
    payload: &ast::Expr,
    source: Option<&ast::Expr>,
    binding: Option<&str>,
    node: &SyntaxNode,
    site: &str,
    proc_macro: bool,
) -> Option<Fact> {
    let foreign = source
        .and_then(foreign)
        .or_else(|| foreign(payload))
        .or_else(|| shape::foreign_binding(payload));
    assess(
        payload,
        foreign,
        binding.is_some_and(|name| bindings::discards(payload, name)),
        node,
        site.to_owned(),
        proc_macro,
    )
}

#[tracing::instrument(level = "trace", skip(expr, node))]
fn assess(
    expr: &ast::Expr,
    foreign: Option<String>,
    discarded: bool,
    node: &SyntaxNode,
    site: String,
    proc_macro: bool,
) -> Option<Fact> {
    let constructor = shape::internal_constructor(expr);
    if foreign.is_some() && constructor.is_some() {
        return Some(Fact {
            node: node.clone(),
            rule: InternalErrorComplianceId::DiscardTyped001,
            snippet: site,
            foreign,
            constructor,
        });
    }
    if (discarded || foreign.is_some() && shape::contains_to_string(expr))
        && !(proc_macro && shape::builds_syn_error(expr))
    {
        return Some(Fact {
            node: node.clone(),
            rule: InternalErrorComplianceId::StringifyForeign001,
            snippet: if discarded {
                format!("{site} — error converted to text without retaining its source")
            } else {
                site
            },
            foreign,
            constructor: None,
        });
    }
    None
}

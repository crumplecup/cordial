//! Function-signature policy, including cfg alternatives and ABI exemptions.
use super::{AntipatternCandidate, AntipatternRuleId, Collector, Context, compact};
use crate::CordialResult;
use ra_ap_syntax::{
    AstNode, SyntaxNode, ast,
    ast::{HasAttrs, HasName},
};
use std::collections::{HashMap, HashSet};

#[tracing::instrument(level = "debug", skip(node))]
pub(super) fn siblings(node: &SyntaxNode) -> HashMap<String, HashSet<String>> {
    let mut groups: HashMap<String, Vec<ast::Fn>> = HashMap::new();
    for function in node.children().filter_map(ast::Fn::cast) {
        if let Some(name) = function.name() {
            groups
                .entry(name.text().to_string())
                .or_default()
                .push(function);
        }
    }
    groups
        .into_iter()
        .filter(|(_, functions)| {
            functions.len() >= 2
                && functions.iter().all(|function| {
                    function
                        .attrs()
                        .any(|attr| matches!(attr.meta(), Some(ast::Meta::CfgMeta(_))))
                })
        })
        .map(|(name, functions)| {
            let names = functions
                .iter()
                .flat_map(|function| {
                    function
                        .param_list()
                        .into_iter()
                        .flat_map(|list| list.params())
                })
                .filter_map(|param| match param.pat() {
                    Some(ast::Pat::IdentPat(pat)) => pat.name().map(|name| name.text().to_string()),
                    _ => None,
                })
                .filter(|name| !name.starts_with('_'))
                .collect();
            (name, names)
        })
        .collect()
}

impl Collector<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn function(&mut self, function: &ast::Fn, context: &Context) -> CordialResult<()> {
        if context.foreign
            || function.attrs().any(|attr| {
                attr.path().is_some_and(|path| {
                    matches!(
                        compact(path.syntax()).as_str(),
                        "proc_macro" | "proc_macro_derive" | "proc_macro_attribute"
                    )
                })
            })
        {
            return Ok(());
        }
        let opaque = function
            .attrs()
            .any(|attr| compact(attr.syntax()) == "#[logic(opaque)]");
        if opaque
            && function
                .body()
                .and_then(|body| body.stmt_list())
                .is_some_and(|body| {
                    body.statements().next().is_none()
                        && body
                            .tail_expr()
                            .is_some_and(|expr| compact(expr.syntax()) == "dead")
                })
        {
            return Ok(());
        }
        let real_names = context
            .functions
            .last()
            .and_then(|name| context.siblings.get(name));
        for param in function
            .param_list()
            .into_iter()
            .flat_map(|list| list.params())
        {
            if let Some(pattern) = param.pat() {
                for binding in bindings(&pattern) {
                    let Some(name) = binding.name() else {
                        continue;
                    };
                    let text = name.text().to_string();
                    if let Some(unprefixed) = text.strip_prefix('_') {
                        if real_names.is_some_and(|names| names.contains(unprefixed)) {
                            continue;
                        }
                        let site = self.site(
                            AntipatternRuleId::UnusedUnderscoreArg001,
                            context.label(),
                            self.source.line(binding.syntax().text_range().start()),
                            text,
                        )?;
                        self.sites.push(AntipatternCandidate::Record(site));
                    }
                }
            }
        }
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(pattern))]
fn bindings(pattern: &ast::Pat) -> Vec<ast::IdentPat> {
    match pattern {
        ast::Pat::IdentPat(ident) => vec![ident.clone()],
        ast::Pat::RefPat(_)
        | ast::Pat::ParenPat(_)
        | ast::Pat::TuplePat(_)
        | ast::Pat::TupleStructPat(_) => pattern
            .syntax()
            .children()
            .filter_map(ast::Pat::cast)
            .flat_map(|pat| bindings(&pat))
            .collect(),
        ast::Pat::RecordPat(record) => record
            .record_pat_field_list()
            .into_iter()
            .flat_map(|list| list.fields())
            .filter_map(|field| field.pat())
            .flat_map(|pat| bindings(&pat))
            .collect(),
        _ => Vec::new(),
    }
}

//! Caller and zero-location branches proven before initializer selection.
use super::native_bindings::{Counts, aliases};
use super::native_values::{bare, empty_file, file};
use super::replacements::InitializerExpressionFact;
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasName},
};
use std::collections::BTreeSet;
use tracing::instrument;

#[derive(Default)]
pub(super) struct Tuples {
    origins: Vec<Origin>,
}
struct Origin {
    fact: InitializerExpressionFact,
    cfg: Vec<String>,
    condition: Option<(String, Option<String>)>,
    caller: bool,
}

#[instrument(level = "trace", skip(source, item))]
pub(super) fn cfg_text(source: &DatabaseSource<'_>, item: &impl HasAttrs) -> Option<Vec<String>> {
    if item.doc_comments().next().is_some() {
        return None;
    }
    item.attrs()
        .map(|attr| {
            if !matches!(attr.meta(), Some(ast::Meta::CfgMeta(_)))
                && (matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
                    || attr.path().and_then(|path| bare(&path)).as_deref() != Some("cfg"))
            {
                return None;
            }
            Some(source.text(attr.syntax()).to_string())
        })
        .collect()
}

impl Tuples {
    #[instrument(level = "trace", skip(source, block, counts))]
    pub(super) fn collect(
        source: &DatabaseSource<'_>,
        block: &ast::BlockExpr,
        counts: &Counts,
    ) -> Self {
        if counts.opaque() || counts.uses().get("file") != Some(&1) {
            return Self::default();
        }
        let origins: Vec<_> = block
            .stmt_list()
            .into_iter()
            .flat_map(|list| list.statements())
            .filter_map(|stmt| {
                let ast::Stmt::LetStmt(local) = stmt else {
                    return None;
                };
                if local.ty().is_some() || local.let_else().is_some() {
                    return None;
                }
                let ast::Pat::TuplePat(pat) = local.pat()? else {
                    return None;
                };
                let pats: Vec<_> = pat.fields().collect();
                let [ast::Pat::IdentPat(line), ast::Pat::IdentPat(file)] = pats.as_slice() else {
                    return None;
                };
                if line.name()?.text() != "line"
                    || file.name()?.text() != "file"
                    || [line, file].iter().any(|pat| {
                        pat.mut_token().is_some()
                            || pat.ref_token().is_some()
                            || pat.pat().is_some()
                    })
                {
                    return None;
                }
                let cfg = cfg_text(source, &local)?;
                let attrs: Vec<_> = local.attrs().collect();
                let condition = match attrs.as_slice() {
                    [] => None,
                    [attr] => Some(source.cfg_condition(attr)?),
                    _ => return None,
                };
                let (fact, caller) = tuple_fact(&local.initializer()?)?;
                Some(Origin {
                    fact,
                    cfg,
                    condition,
                    caller,
                })
            })
            .collect();
        let complete = counts.names().get("file") == Some(&origins.len())
            && match origins.as_slice() {
                [origin] => origin.caller,
                [left, right] => {
                    (left.caller || right.caller)
                        && match (&left.condition, &right.condition) {
                            (Some((left, left_not)), Some((right, right_not))) => {
                                left_not.as_ref() == Some(right) || right_not.as_ref() == Some(left)
                            }
                            _ => false,
                        }
                }
                _ => false,
            };
        if complete {
            Self { origins }
        } else {
            Self::default()
        }
    }

    #[instrument(level = "trace", skip(self, cfg))]
    pub(super) fn at(
        &self,
        cfg: Option<&[String]>,
        start: usize,
    ) -> Vec<InitializerExpressionFact> {
        let Some(cfg) = cfg else {
            return Vec::new();
        };
        if self.origins.is_empty()
            || self.origins.iter().any(|origin| origin.fact.end() >= start)
            || (self.origins.len() == 1 && self.origins[0].cfg != cfg)
            || (self.origins.len() == 2 && !cfg.is_empty())
        {
            return Vec::new();
        }
        self.origins
            .iter()
            .map(|origin| origin.fact.clone())
            .collect()
    }
}

#[instrument(level = "trace", skip(expr))]
fn tuple_fact(expr: &ast::Expr) -> Option<(InitializerExpressionFact, bool)> {
    let (tuple, names) = match expr {
        ast::Expr::BlockExpr(block)
            if block.attrs().next().is_none()
                && block.doc_comments().next().is_none()
                && block.label().is_none()
                && block.modifier().is_none() =>
        {
            let ast::Expr::TupleExpr(tuple) = block.stmt_list()?.tail_expr()? else {
                return None;
            };
            (tuple, aliases(block, &Counts::collect(block)))
        }
        ast::Expr::TupleExpr(tuple) => (tuple.clone(), BTreeSet::new()),
        _ => return None,
    };
    let expressions: Vec<_> = tuple.fields().collect();
    let [line, value] = expressions.as_slice() else {
        return None;
    };
    if matches!(line, ast::Expr::Literal(lit) if matches!(lit.kind(), ast::LiteralKind::IntNumber(number) if number.value().ok() == Some(0)))
    {
        return empty_file(value).map(|fact| (fact, false));
    }
    let ast::Expr::MethodCallExpr(method) = line else {
        return None;
    };
    if method.name_ref()?.text() != "line" || method.arg_list()?.args().next().is_some() {
        return None;
    }
    let ast::Expr::PathExpr(path) = method.receiver()? else {
        return None;
    };
    if !names.contains(&bare(&path.path()?)?) {
        return None;
    }
    file(value, "file", false, &names).map(|fact| (fact, true))
}

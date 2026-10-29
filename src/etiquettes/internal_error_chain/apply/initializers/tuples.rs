//! Proven tuple origins, including exhaustive complementary configuration branches.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, FieldValue, Meta, Pat};
use tracing::instrument;

use super::bindings::{BindingCounts, caller_aliases};
use super::replacements::{InitializerExpressionFact, inspect_empty_file, inspect_initializer};
use super::{FieldKind, cfg_text, span_range};

#[derive(Clone, Debug, Default)]
pub(super) struct TupleFileOrigins {
    origins: Vec<TupleFileOrigin>,
}

#[derive(Clone, Debug)]
struct TupleFileOrigin {
    fact: InitializerExpressionFact,
    cfg: Vec<String>,
    condition: Option<Meta>,
    caller: bool,
}

impl TupleFileOrigins {
    /// Every accepted branch must dominate this use in the same configuration.
    #[instrument(level = "trace", skip(self, source, field))]
    pub(super) fn at(
        &self,
        source: &str,
        field: &FieldValue,
    ) -> Option<Vec<InitializerExpressionFact>> {
        let cfg = cfg_text(source, &field.attrs)?;
        let (start, _) = span_range(source, field.expr.span())?;
        if self.origins.is_empty()
            || self.origins.iter().any(|origin| origin.fact.end() >= start)
            || (self.origins.len() == 1 && self.origins[0].cfg != cfg)
            || (self.origins.len() == 2 && !cfg.is_empty())
        {
            return None;
        }
        Some(
            self.origins
                .iter()
                .map(|origin| origin.fact.clone())
                .collect(),
        )
    }
}

/// Collect once per constructor from the shared tree; never infer active cfgs.
#[instrument(level = "debug", skip(source, block, bindings))]
pub(super) fn tuple_file_origins(
    source: &str,
    block: &syn::Block,
    bindings: &BindingCounts,
) -> TupleFileOrigins {
    if bindings.opaque_local_names() || bindings.uses().get("file") != Some(&1) {
        return TupleFileOrigins::default();
    }
    let origins: Vec<_> = block
        .stmts
        .iter()
        .filter_map(|stmt| {
            let syn::Stmt::Local(local) = stmt else {
                return None;
            };
            let Pat::Tuple(pat) = &local.pat else {
                return None;
            };
            let mut elements = pat.elems.iter();
            let (Some(Pat::Ident(line)), Some(Pat::Ident(file)), None) =
                (elements.next(), elements.next(), elements.next())
            else {
                return None;
            };
            if line.ident != "line"
                || file.ident != "file"
                || [line, file].iter().any(|binding| {
                    binding.mutability.is_some()
                        || binding.by_ref.is_some()
                        || binding.subpat.is_some()
                })
            {
                return None;
            }
            let cfg = cfg_text(source, &local.attrs)?;
            let condition = match local.attrs.as_slice() {
                [] => None,
                [attr] => Some(attr.parse_args::<Meta>().ok()?),
                _ => return None,
            };
            let init = local.init.as_ref()?;
            if init.diverge.is_some() {
                return None;
            }
            let (fact, caller) = tuple_fact(source, &init.expr)?;
            Some(TupleFileOrigin {
                fact,
                cfg,
                condition,
                caller,
            })
        })
        .collect();
    let complete = bindings.names().get("file") == Some(&origins.len())
        && match origins.as_slice() {
            [origin] => origin.caller,
            [left, right] => {
                (left.caller || right.caller)
                    && complementary(left.condition.as_ref(), right.condition.as_ref())
            }
            _ => false,
        };
    if complete {
        TupleFileOrigins { origins }
    } else {
        TupleFileOrigins::default()
    }
}

#[instrument(level = "trace", skip(left, right))]
fn complementary(left: Option<&Meta>, right: Option<&Meta>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    let negates = |negated: &Meta, other: &Meta| {
        matches!(negated, Meta::List(list) if list.path.is_ident("not")
            && list.parse_args::<Meta>().is_ok_and(|inner| inner == *other))
    };
    negates(left, right) || negates(right, left)
}

#[instrument(level = "trace", skip(source, expr))]
fn tuple_fact(source: &str, expr: &Expr) -> Option<(InitializerExpressionFact, bool)> {
    let (tuple, aliases) = match expr {
        Expr::Block(init) if init.attrs.is_empty() && init.label.is_none() => {
            let Some(syn::Stmt::Expr(Expr::Tuple(tuple), None)) = init.block.stmts.last() else {
                return None;
            };
            let mut bindings = BindingCounts::default();
            bindings.visit_block(&init.block);
            (tuple, caller_aliases(&init.block, &bindings))
        }
        Expr::Tuple(tuple) => (tuple, BTreeSet::new()),
        _ => return None,
    };
    let mut expressions = tuple.elems.iter();
    let (Some(line), Some(file), None) =
        (expressions.next(), expressions.next(), expressions.next())
    else {
        return None;
    };
    if matches!(line, Expr::Lit(lit) if matches!(&lit.lit, syn::Lit::Int(value) if value.base10_digits() == "0"))
    {
        return inspect_empty_file(source, file).map(|fact| (fact, false));
    }
    let Expr::MethodCall(method) = line else {
        return None;
    };
    if method.method != "line"
        || !method.args.is_empty()
        || !matches!(&*method.receiver, Expr::Path(path)
            if path.qself.is_none() && path.path.get_ident().is_some_and(|name| aliases.contains(&name.to_string())))
    {
        return None;
    }
    inspect_initializer(
        source,
        file,
        "file",
        false,
        FieldKind::CallSiteFile,
        &aliases,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
    )
    .map(|fact| (fact, true))
}

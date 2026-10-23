use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprClosure, ExprMethodCall, Fields, FnArg, Pat, Signature, Type};

use super::{Conversion, SignatureChainIndex, Ty};

#[tracing::instrument(level = "debug")]
pub(super) fn scoped(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}

#[tracing::instrument(level = "debug", skip(generics))]
pub(super) fn type_params(generics: &syn::Generics) -> Vec<String> {
    generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(ty) => Some(ty.ident.to_string()),
            _ => None,
        })
        .collect()
}

#[tracing::instrument(level = "debug", skip(fields))]
pub(super) fn source_fields(fields: &Fields) -> Vec<Type> {
    fields
        .iter()
        .filter(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("source") || attr.path().is_ident("from"))
                || field.ident.as_ref().is_some_and(|ident| ident == "source")
        })
        .map(|field| field.ty.clone())
        .collect()
}

#[tracing::instrument(level = "debug", skip(method))]
pub(super) fn source_returns_named_field(method: &syn::ImplItemFn) -> bool {
    let Some(syn::Stmt::Expr(Expr::Call(call), _)) = method.block.stmts.last() else {
        return false;
    };
    let Expr::Path(function) = call.func.as_ref() else {
        return false;
    };
    if !function.path.is_ident("Some") || call.args.len() != 1 {
        return false;
    }
    let Some(Expr::Reference(reference)) = call.args.first() else {
        return false;
    };
    let Expr::Field(field) = reference.expr.as_ref() else {
        return false;
    };
    let Expr::Path(base) = field.base.as_ref() else {
        return false;
    };
    base.path.is_ident("self")
        && matches!(&field.member, syn::Member::Named(name) if name == "source")
}

#[tracing::instrument(level = "debug", skip(method))]
pub(super) fn source_returns_variants(method: &syn::ImplItemFn) -> Option<Vec<String>> {
    let syn::Stmt::Expr(Expr::Match(matched), _) = method.block.stmts.last()? else {
        return None;
    };
    let Expr::Path(scrutinee) = matched.expr.as_ref() else {
        return None;
    };
    if !scrutinee.path.is_ident("self") {
        return None;
    }
    let mut variants = Vec::new();
    for arm in &matched.arms {
        if let Expr::Path(none) = arm.body.as_ref() {
            if none.path.is_ident("None") {
                continue;
            }
        }
        let Expr::Call(some) = arm.body.as_ref() else {
            return None;
        };
        let Expr::Path(function) = some.func.as_ref() else {
            return None;
        };
        if !function.path.is_ident("Some") || some.args.len() != 1 {
            return None;
        }
        let Expr::Path(source) = some.args.first()? else {
            return None;
        };
        let binding = source.path.get_ident()?.to_string();
        let variant = match &arm.pat {
            Pat::TupleStruct(pattern) if pattern.elems.len() == 1 => {
                let Pat::Ident(value) = pattern.elems.first()? else {
                    return None;
                };
                if value.ident != binding {
                    return None;
                }
                pattern.path.segments.last()?.ident.to_string()
            }
            Pat::Struct(pattern) => {
                let field = pattern.fields.iter().find(
                    |field| matches!(field.pat.as_ref(), Pat::Ident(value) if value.ident == binding),
                )?;
                if !matches!(&field.member, syn::Member::Named(name) if name == "source") {
                    return None;
                }
                pattern.path.segments.last()?.ident.to_string()
            }
            _ => return None,
        };
        variants.push(variant);
    }
    Some(variants)
}

#[tracing::instrument(level = "trace", skip(expr), ret)]
pub(super) fn is_binding(expr: &Expr, binding: &str) -> bool {
    matches!(expr, Expr::Path(path) if path.path.is_ident(binding))
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn has_bound_call(expr: &Expr, binding: &str) -> bool {
    struct BoundCall<'a> {
        binding: &'a str,
        found: bool,
    }
    impl<'ast> Visit<'ast> for BoundCall<'_> {
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if call.args.iter().any(|arg| is_binding(arg, self.binding)) {
                self.found = true;
            }
            syn::visit::visit_expr_call(self, call);
        }
    }
    let mut visitor = BoundCall {
        binding,
        found: false,
    };
    visitor.visit_expr(expr);
    visitor.found
}

#[tracing::instrument(level = "debug", skip(formal, actual, generics, bindings))]
pub(super) fn bind_generics(
    formal: &Type,
    actual: &Ty,
    generics: &BTreeSet<String>,
    bindings: &mut BTreeMap<String, Ty>,
) -> Option<()> {
    let Type::Path(path) = formal else {
        return Some(());
    };
    if path.path.segments.len() == 1 && generics.contains(&path.path.segments[0].ident.to_string())
    {
        bindings.insert(path.path.segments[0].ident.to_string(), actual.clone());
    }
    Some(())
}

struct ConversionVisitor {
    scope: Vec<String>,
    file: PathBuf,
    params: BTreeMap<String, Type>,
    conversions: Vec<Conversion>,
}

impl SignatureChainIndex {
    #[tracing::instrument(level = "debug", skip(self, block, sig, file))]
    pub(super) fn collect_calls(
        &mut self,
        block: &syn::Block,
        sig: &Signature,
        scope: &[String],
        file: &Path,
    ) {
        let params = sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(typed) => match typed.pat.as_ref() {
                    Pat::Ident(ident) => Some((ident.ident.to_string(), (*typed.ty).clone())),
                    _ => None,
                },
                FnArg::Receiver(_) => None,
            })
            .collect();
        let mut visitor = ConversionVisitor {
            scope: scope.to_vec(),
            file: file.to_path_buf(),
            params,
            conversions: Vec::new(),
        };
        visitor.visit_block(block);
        self.conversions.extend(visitor.conversions);
    }
}

impl<'ast> Visit<'ast> for ConversionVisitor {
    #[tracing::instrument(level = "debug", skip(self, call))]
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if call.method == "map_err"
            && let Some(Expr::Closure(ExprClosure { inputs, body, .. })) = call.args.first()
            && let Some(Pat::Ident(binding)) = inputs.first()
        {
            self.conversions.push(Conversion {
                scope: self.scope.clone(),
                receiver: (*call.receiver).clone(),
                body: (**body).clone(),
                binding: binding.ident.to_string(),
                params: self.params.clone(),
                file: self.file.clone(),
                line: call.span().start().line as u32,
            });
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}

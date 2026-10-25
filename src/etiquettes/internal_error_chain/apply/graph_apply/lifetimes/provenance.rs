use std::collections::BTreeSet;

use syn::Type;
use syn::visit::Visit;
use tracing::instrument;

use super::contains_lifetime_type;

#[instrument(level = "debug", skip(output, names))]
pub(super) fn returns_graph_type(output: &syn::ReturnType, names: &BTreeSet<String>) -> bool {
    struct GraphTypeUse<'a> {
        names: &'a BTreeSet<String>,
        found: bool,
    }
    impl<'ast> Visit<'ast> for GraphTypeUse<'_> {
        fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
            self.found |= path
                .path
                .segments
                .last()
                .is_some_and(|segment| self.names.contains(&segment.ident.to_string()));
            syn::visit::visit_type_path(self, path);
        }
    }
    let mut visitor = GraphTypeUse {
        names,
        found: false,
    };
    visitor.visit_return_type(output);
    visitor.found
}

#[instrument(level = "debug", skip(signature))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn anchored_parameters(
    signature: &syn::Signature,
    lifetime: &str,
) -> BTreeSet<String> {
    fn collect(pat: &syn::Pat, ty: &Type, lifetime: &str, names: &mut BTreeSet<String>) {
        match (pat, ty) {
            (syn::Pat::Ident(ident), ty) if contains_lifetime_type(ty, lifetime) => {
                names.insert(ident.ident.to_string());
            }
            (syn::Pat::Tuple(pat), Type::Tuple(ty)) if pat.elems.len() == ty.elems.len() => {
                for (pat, ty) in pat.elems.iter().zip(&ty.elems) {
                    collect(pat, ty, lifetime, names);
                }
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for input in &signature.inputs {
        if let syn::FnArg::Typed(input) = input {
            collect(&input.pat, &input.ty, lifetime, &mut names);
        }
    }
    names
}

#[instrument(level = "debug", skip(expr, parameters))]
fn anchored_expr(expr: &syn::Expr, parameters: &BTreeSet<String>) -> bool {
    match expr {
        syn::Expr::Path(path) if path.path.segments.len() == 1 => {
            parameters.contains(&path.path.segments[0].ident.to_string())
        }
        syn::Expr::Reference(reference) => anchored_expr(&reference.expr, parameters),
        syn::Expr::MethodCall(call)
            if call.args.is_empty()
                && matches!(
                    call.method.to_string().as_str(),
                    "clone" | "to_owned" | "to_string"
                ) =>
        {
            anchored_expr(&call.receiver, parameters)
        }
        syn::Expr::Call(call)
            if matches!(&*call.func, syn::Expr::Path(path)
                if path.path.segments.len() == 2
                    && path.path.segments[0].ident == "Box"
                    && path.path.segments[1].ident == "new")
                && call.args.len() == 1 =>
        {
            call.args
                .first()
                .is_some_and(|arg| anchored_expr(arg, parameters))
        }
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr, names, parameters))]
fn direct_graph_return(
    expr: &syn::Expr,
    names: &BTreeSet<String>,
    parameters: &BTreeSet<String>,
) -> bool {
    let syn::Expr::Call(call) = expr else {
        return false;
    };
    let syn::Expr::Path(path) = &*call.func else {
        return false;
    };
    path.path.segments.len() >= 2
        && path
            .path
            .segments
            .iter()
            .rev()
            .nth(1)
            .is_some_and(|segment| names.contains(&segment.ident.to_string()))
        && call.args.iter().all(|arg| anchored_expr(arg, parameters))
}

#[instrument(level = "debug", skip(expr, parameters))]
pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn constructor_origin(
    expr: &syn::Expr,
    parameters: &BTreeSet<String>,
) -> bool {
    if anchored_expr(expr, parameters) {
        return true;
    }
    let syn::Expr::Call(call) = expr else {
        return false;
    };
    matches!(&*call.func, syn::Expr::Path(path)
        if path.path.segments.len() == 2
            && path.path.segments[0].ident == "Box"
            && path.path.segments[1].ident == "new")
        && call.args.len() == 1
        && call
            .args
            .first()
            .is_some_and(|arg| constructor_origin(arg, parameters))
}

#[instrument(level = "debug", skip(item))]
pub(super) fn safe_impl_constructor(item: &syn::ImplItemFn, lifetime: &str) -> bool {
    let parameters = anchored_parameters(&item.sig, lifetime);
    if parameters.is_empty() {
        return false;
    }
    let [syn::Stmt::Expr(tail, None)] = item.block.stmts.as_slice() else {
        return false;
    };
    match tail {
        syn::Expr::Struct(value) if value.path.is_ident("Self") && value.rest.is_none() => {
            value.fields.iter().all(|field| {
                !matches!(&field.member, syn::Member::Named(name)
                    if name == "kind" || name == "source")
                    || constructor_origin(&field.expr, &parameters)
            })
        }
        syn::Expr::Call(call)
            if matches!(&*call.func, syn::Expr::Path(path)
                if path.path.segments.len() == 2
                    && path.path.segments[0].ident == "Self") =>
        {
            call.args.iter().all(|arg| anchored_expr(arg, &parameters))
        }
        _ => false,
    }
}

#[instrument(level = "debug", skip(block, parameters))]
fn forwarding_result(block: &syn::Block, parameters: &BTreeSet<String>) -> bool {
    let [
        syn::Stmt::Local(local),
        syn::Stmt::Expr(syn::Expr::Try(try_expr), Some(_)),
        syn::Stmt::Expr(syn::Expr::Call(ok), None),
    ] = block.stmts.as_slice()
    else {
        return false;
    };
    let syn::Pat::Type(binding) = &local.pat else {
        return false;
    };
    let syn::Pat::Ident(binding_name) = &*binding.pat else {
        return false;
    };
    if !matches!(&*binding.ty, Type::Path(path)
        if path.path.segments.last().is_some_and(|segment| segment.ident == "Result"))
        || !matches!(&*try_expr.expr, syn::Expr::Path(path)
            if path.path.is_ident(&binding_name.ident))
    {
        return false;
    }
    let Some(init) = &local.init else {
        return false;
    };
    let syn::Expr::Call(err) = &*init.expr else {
        return false;
    };
    let [syn::Expr::Tuple(tuple)] = err.args.iter().collect::<Vec<_>>().as_slice() else {
        return false;
    };
    matches!(&*err.func, syn::Expr::Path(path) if path.path.is_ident("Err"))
        && tuple.elems.iter().all(|arg| anchored_expr(arg, parameters))
        && matches!(&*ok.func, syn::Expr::Path(path) if path.path.is_ident("Ok"))
        && ok.args.len() == 1
        && matches!(ok.args.first(), Some(syn::Expr::Tuple(unit)) if unit.elems.is_empty())
}

#[instrument(level = "debug", skip(item, names))]
pub(super) fn safe_top_level_return(
    item: &syn::ItemFn,
    names: &BTreeSet<String>,
    lifetime: &str,
) -> bool {
    let parameters = anchored_parameters(&item.sig, lifetime);
    if parameters.is_empty() {
        return false;
    }
    matches!(item.block.stmts.as_slice(), [syn::Stmt::Expr(expr, None)]
        if direct_graph_return(expr, names, &parameters))
        || forwarding_result(&item.block, &parameters)
}

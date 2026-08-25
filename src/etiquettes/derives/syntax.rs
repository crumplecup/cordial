use syn::{
    Attribute, Block, Expr, FnArg, ItemImpl, Pat, ReturnType, Signature, Stmt, Type, Visibility,
};

use tracing::instrument;
#[instrument(level = "debug", skip(vis))]
pub(super) fn field_is_exposed(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

#[instrument(level = "trace", skip(attrs), ret)]
pub(super) fn has_track_caller(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("track_caller"))
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn constructor_arg_count(sig: &Signature) -> usize {
    sig.inputs
        .iter()
        .filter(|arg| !matches!(arg, FnArg::Receiver(_)))
        .count()
}

#[instrument(level = "debug", skip(item_impl))]
pub(super) fn error_impl_target(item_impl: &ItemImpl) -> Option<String> {
    let (_, trait_path, _) = item_impl.trait_.as_ref()?;
    let last = trait_path.segments.last()?;
    if last.ident != "Error" {
        return None;
    }
    Some(type_label(&item_impl.self_ty))
}

#[instrument(level = "trace", skip(attrs))]
pub(super) fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

/// Clap schema types: each field is a CLI argument, not an encapsulated record.
#[instrument(level = "trace", skip(attrs), ret)]
pub(super) fn is_clap_schema(attrs: &[Attribute]) -> bool {
    has_derive(attrs, "Parser") || has_derive(attrs, "Args") || has_derive(attrs, "Subcommand")
}

#[instrument(level = "trace", skip(attrs))]
pub(super) fn has_derive(attrs: &[Attribute], needle: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("derive") {
            return false;
        }
        let tokens = list.tokens.to_string();
        tokens.split(',').any(|part| {
            let compact = part.replace(' ', "");
            compact == needle
                || compact.ends_with(&format!("::{needle}"))
                || compact.contains(&format!("::{needle}::"))
        })
    })
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn consumes_self(sig: &Signature) -> bool {
    matches!(sig.receiver(), Some(recv) if recv.reference.is_none())
}

#[instrument(level = "trace", skip(sig))]
pub(super) fn is_fluent_setter(sig: &Signature) -> bool {
    if !matches!(sig.output, ReturnType::Type(_, _)) {
        return false;
    }
    let Some(recv) = sig.receiver() else {
        return false;
    };
    if recv.reference.is_some() {
        return false;
    }
    recv.mutability.is_some() && sig.inputs.len() >= 2
}

/// How a getter body reads a field — each maps to a derive option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldRead {
    /// `&self.field` — a plain `#[derive(Getters)]` returns exactly this.
    Direct,
    /// Bare `self.field` — only compiles when the field is `Copy`, and
    /// needs `#[getter(copy)]` since a plain `#[derive(Getters)]`
    /// generates a reference-returning getter, not a copy-out one.
    DirectOwned,
    Clone,
    AsStr,
    AsRef,
}

#[instrument(level = "debug", skip(block), ret)]
pub(super) fn classify_field_read(block: &Block) -> Option<(String, FieldRead)> {
    let stmts = non_item_stmts(block);
    if stmts.len() != 1 {
        return None;
    }
    expr_field_read(stmt_tail_expr(stmts[0])?)
}

/// Setter body that `derive_setters` can emit, including `into` and `strip_option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetterShape {
    Assign,
    Into,
    StripOption,
    StripOptionInto,
}

impl SetterShape {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn recommendation(self) -> &'static str {
        match self {
            Self::Assign => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\")]"
            }
            Self::Into => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", into)]"
            }
            Self::StripOption => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", strip_option)]"
            }
            Self::StripOptionInto => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", strip_option, into)]"
            }
        }
    }
}

#[instrument(level = "debug", skip(block, sig), ret)]
pub(super) fn classify_setter_body(
    block: &Block,
    field_name: &str,
    sig: &Signature,
) -> Option<SetterShape> {
    let params = value_param_names(sig);
    if params.len() != 1 {
        return None;
    }
    let stmts = non_item_stmts(block);
    let assign = match stmts.as_slice() {
        [assign] => *assign,
        [assign, ret] if stmt_is_return_self(ret) => *assign,
        _ => return None,
    };
    stmt_setter_shape(assign, field_name, &params)
}

#[instrument(level = "debug", skip(sig))]
fn value_param_names(sig: &Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            let FnArg::Typed(pat_type) = arg else {
                return None;
            };
            let Pat::Ident(ident) = &*pat_type.pat else {
                return None;
            };
            Some(ident.ident.to_string())
        })
        .collect()
}

#[instrument(level = "debug", skip(stmt, params), ret)]
fn stmt_setter_shape(stmt: &Stmt, field_name: &str, params: &[String]) -> Option<SetterShape> {
    let Expr::Assign(assign) = stmt_tail_expr(stmt)? else {
        return None;
    };
    if !expr_is_self_field(&assign.left, field_name) {
        return None;
    }
    classify_setter_rhs(&assign.right, params)
}

#[instrument(level = "debug", skip(stmt), ret)]
fn stmt_is_return_self(stmt: &Stmt) -> bool {
    let Some(expr) = stmt_tail_expr(stmt) else {
        return false;
    };
    match expr {
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .is_some_and(|inner| expr_is_self(inner)),
        other => expr_is_self(other),
    }
}

#[instrument(level = "debug", skip(expr, params), ret)]
fn classify_setter_rhs(expr: &Expr, params: &[String]) -> Option<SetterShape> {
    match expr {
        Expr::Call(call) if expr_is_some_ctor(&call.func) && call.args.len() == 1 => {
            match classify_owned_input(&call.args[0], params)? {
                OwnedInput::Direct => Some(SetterShape::StripOption),
                OwnedInput::Into => Some(SetterShape::StripOptionInto),
            }
        }
        Expr::Paren(paren) => classify_setter_rhs(&paren.expr, params),
        Expr::Group(group) => classify_setter_rhs(&group.expr, params),
        other => match classify_owned_input(other, params)? {
            OwnedInput::Direct => Some(SetterShape::Assign),
            OwnedInput::Into => Some(SetterShape::Into),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedInput {
    Direct,
    Into,
}

#[instrument(level = "debug", skip(expr, params), ret)]
fn classify_owned_input(expr: &Expr, params: &[String]) -> Option<OwnedInput> {
    match expr {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| params.iter().any(|param| ident == param))
            .then_some(OwnedInput::Direct),
        Expr::MethodCall(call)
            if call.args.is_empty()
                && matches!(
                    call.method.to_string().as_str(),
                    "into" | "clone" | "to_owned" | "to_string"
                ) =>
        {
            classify_owned_input(&call.receiver, params).map(|_| OwnedInput::Into)
        }
        Expr::Paren(paren) => classify_owned_input(&paren.expr, params),
        Expr::Group(group) => classify_owned_input(&group.expr, params),
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_is_some_ctor(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Some")
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_field_read(expr: &Expr) -> Option<(String, FieldRead)> {
    match expr {
        // `&self.field` and bare `self.field` parse to the same
        // `Expr::Field` once unwrapped, but they aren't the same getter:
        // the former borrows (a plain `#[derive(Getters)]` matches it),
        // the latter moves the field out by value, which only compiles
        // when the field is `Copy` and needs `#[getter(copy)]` -- so the
        // `&` has to be read *before* recursing, not discarded.
        Expr::Reference(reference) => {
            let (name, _) = expr_field_read(&reference.expr)?;
            Some((name, FieldRead::Direct))
        }
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .and_then(|inner| expr_field_read(inner)),
        Expr::Paren(paren) => expr_field_read(&paren.expr),
        Expr::Group(group) => expr_field_read(&group.expr),
        Expr::Field(field) => {
            let name = field_member_name(&field.member)?;
            expr_is_self(&field.base).then_some((name, FieldRead::DirectOwned))
        }
        Expr::MethodCall(call) if call.args.is_empty() => {
            let kind = match call.method.to_string().as_str() {
                "clone" | "to_owned" => FieldRead::Clone,
                "as_str" => FieldRead::AsStr,
                "as_ref" => FieldRead::AsRef,
                _ => return None,
            };
            let (field, inner) = expr_field_read(&call.receiver)?;
            matches!(inner, FieldRead::Direct | FieldRead::DirectOwned).then_some((field, kind))
        }
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_is_self_field(expr: &Expr, field_name: &str) -> bool {
    match expr {
        Expr::Field(field) => {
            field_member_name(&field.member).as_deref() == Some(field_name)
                && expr_is_self(&field.base)
        }
        Expr::Paren(paren) => expr_is_self_field(&paren.expr, field_name),
        Expr::Group(group) => expr_is_self_field(&group.expr, field_name),
        _ => false,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_is_self(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path.path.is_ident("self"),
        Expr::Paren(paren) => expr_is_self(&paren.expr),
        Expr::Group(group) => expr_is_self(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(block))]
pub(super) fn body_is_struct_literal(block: &Block, type_name: &str) -> bool {
    let stmts = non_item_stmts(block);
    if stmts.is_empty() || stmts.len() > 2 {
        return false;
    }
    stmts.iter().any(|stmt| {
        stmt_tail_expr(stmt).is_some_and(|expr| expr_is_struct_literal(expr, type_name))
    })
}

#[instrument(level = "debug", skip(block))]
fn non_item_stmts(block: &Block) -> Vec<&Stmt> {
    block
        .stmts
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Item(_)))
        .collect()
}

#[instrument(level = "debug", skip(stmt))]
fn stmt_tail_expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr))]
fn expr_is_struct_literal(expr: &Expr, type_name: &str) -> bool {
    match expr {
        Expr::Struct(item) => type_matches(&item.path, type_name) || path_is_self(&item.path),
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .is_some_and(|inner| expr_is_struct_literal(inner, type_name)),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_self(path: &syn::Path) -> bool {
    path.is_ident("Self")
}

#[instrument(level = "debug", skip(member))]
fn field_member_name(member: &syn::Member) -> Option<String> {
    match member {
        syn::Member::Named(ident) => Some(ident.to_string()),
        syn::Member::Unnamed(_) => None,
    }
}

#[instrument(level = "debug", skip(path))]
fn type_matches(path: &syn::Path, type_name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == type_name)
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

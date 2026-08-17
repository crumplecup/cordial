use syn::{Attribute, Block, Expr, ReturnType, Signature, Stmt, Type, Visibility};

pub(super) fn field_is_exposed(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

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
            let trimmed = part.trim();
            trimmed == needle
                || trimmed.ends_with(&format!("::{needle}"))
                || trimmed.contains(&format!("::{needle}::"))
        })
    })
}

pub(super) fn consumes_self(sig: &Signature) -> bool {
    matches!(sig.receiver(), Some(recv) if recv.reference.is_none())
}

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

pub(super) fn body_is_trivial_field_access(block: &Block, field_name: &str) -> bool {
    let stmts = non_item_stmts(block);
    if stmts.len() != 1 {
        return false;
    }
    let Some(expr) = stmt_tail_expr(stmts[0]) else {
        return false;
    };
    expr_is_field_access(expr, field_name)
}

pub(super) fn body_is_struct_literal(block: &Block, type_name: &str) -> bool {
    let stmts = non_item_stmts(block);
    if stmts.is_empty() || stmts.len() > 2 {
        return false;
    }
    stmts.iter().any(|stmt| {
        stmt_tail_expr(stmt).is_some_and(|expr| expr_is_struct_literal(expr, type_name))
    })
}

fn non_item_stmts(block: &Block) -> Vec<&Stmt> {
    block
        .stmts
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Item(_)))
        .collect()
}

fn stmt_tail_expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

fn expr_is_field_access(expr: &Expr, field_name: &str) -> bool {
    match expr {
        Expr::Field(field) => field_member_name(&field.member).as_deref() == Some(field_name),
        Expr::Reference(reference) => expr_is_field_access(&reference.expr, field_name),
        Expr::MethodCall(call) if call.method == "clone" => {
            expr_is_field_access(&call.receiver, field_name)
        }
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .is_some_and(|inner| expr_is_field_access(inner, field_name)),
        _ => false,
    }
}

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

fn path_is_self(path: &syn::Path) -> bool {
    path.is_ident("Self")
}

fn field_member_name(member: &syn::Member) -> Option<String> {
    match member {
        syn::Member::Named(ident) => Some(ident.to_string()),
        syn::Member::Unnamed(_) => None,
    }
}

fn type_matches(path: &syn::Path, type_name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == type_name)
}

pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

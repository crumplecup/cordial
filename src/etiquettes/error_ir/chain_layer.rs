//! Error-chain layer: preserved vs. discarded foreign error chains.
//!
//! Gated as a whole unit by `#[cfg(feature = "error_chain")]` on the `mod
//! chain_layer;` declaration in `error_ir/mod.rs` — nothing inside this file
//! needs its own `#[cfg]`, since the entire file only compiles when the
//! feature is enabled.

use syn::spanned::Spanned;
use syn::{
    Expr, ExprClosure, ExprMethodCall, ExprPath, ExprTry, Fields, GenericArgument, ItemEnum,
    ItemImpl, ItemStruct, Member, PathArguments, ReturnType, Type, TypePath,
};

use super::visitor::{SiteCtx, raw_expr_snippet, truncate_snippet};
use crate::etiquettes::error_chain::{ErrorChainProbeId, ErrorChainRecord};
use crate::etiquettes::error_sites::infer_foreign_error_type;

/// Per-file accumulator for the `error_chain` layer.
#[derive(Default)]
pub(super) struct ChainLayer {
    fn_return_type: Option<String>,
    chain: Vec<ErrorChainRecord>,
}

impl ChainLayer {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Enter a fn/method body, returning the previous return-type label so
    /// the caller can restore it via [`Self::exit_fn`].
    pub(super) fn enter_fn(&mut self, output: &ReturnType) -> Option<String> {
        let prev = self.fn_return_type.take();
        self.fn_return_type = return_type_label(output);
        prev
    }

    pub(super) fn exit_fn(&mut self, prev: Option<String>) {
        self.fn_return_type = prev;
    }

    fn push(
        &mut self,
        rule_id: ErrorChainProbeId,
        line: u32,
        snippet: String,
        foreign_error_type: Option<String>,
        ctx: &SiteCtx,
    ) {
        self.chain.push(ErrorChainRecord {
            rule_id,
            context: ctx.context.clone(),
            file: ctx.rel_file.clone(),
            line,
            snippet,
            foreign_error_type,
        });
    }

    pub(super) fn on_item_struct(&mut self, item_struct: &ItemStruct, ctx: &SiteCtx) {
        let Fields::Named(fields) = &item_struct.fields else {
            return;
        };
        for field in &fields.named {
            let Some(ident) = &field.ident else {
                continue;
            };
            if ident != "source" {
                continue;
            }
            if is_string_type(&field.ty) {
                continue;
            }
            self.push(
                ErrorChainProbeId::WrapperSourceField001,
                field.span().start().line as u32,
                format!(
                    "struct {} {{ source: {} }}",
                    item_struct.ident,
                    type_label(&field.ty)
                ),
                foreign_type_from_rust_type(&field.ty),
                ctx,
            );
        }
    }

    pub(super) fn on_item_enum(&mut self, item_enum: &ItemEnum, ctx: &SiteCtx) {
        if !enum_name_suggests_error_kind(&item_enum.ident.to_string()) {
            return;
        }
        for variant in &item_enum.variants {
            let payload = match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
                Fields::Named(fields) if fields.named.len() == 1 => &fields.named[0].ty,
                _ => continue,
            };
            if is_string_type(payload) {
                continue;
            }
            self.push(
                ErrorChainProbeId::KindWrapperPayload001,
                variant.span().start().line as u32,
                format!(
                    "enum {} {{ {}({}) }}",
                    item_enum.ident,
                    variant.ident,
                    type_label(payload)
                ),
                foreign_type_from_rust_type(payload),
                ctx,
            );
        }
    }

    pub(super) fn on_item_impl(&mut self, item_impl: &ItemImpl, ctx: &SiteCtx) {
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            return;
        };
        let Some(from_type) = extract_from_source_type(trait_path) else {
            return;
        };
        if is_string_type(from_type) || !is_foreign_rust_type(from_type) {
            return;
        }
        self.push(
            ErrorChainProbeId::FromBridge001,
            item_impl.span().start().line as u32,
            format!(
                "impl From<{}> for {}",
                type_label(from_type),
                type_label(&item_impl.self_ty)
            ),
            foreign_type_from_rust_type(from_type),
            ctx,
        );
    }

    pub(super) fn on_map_err(&mut self, call: &ExprMethodCall, ctx: &SiteCtx) {
        if !return_type_is_umbrella(&self.fn_return_type) {
            return;
        }
        if let Some((foreign_type, source_snippet)) =
            preserved_map_err_conversion(call, &call.receiver)
        {
            self.push(
                ErrorChainProbeId::PreservedMapErr001,
                call.span().start().line as u32,
                format!("{source_snippet}.map_err(…)"),
                Some(foreign_type),
                ctx,
            );
        }
    }

    pub(super) fn on_expr_try(&mut self, node: &ExprTry, ctx: &SiteCtx) {
        if !try_propagates_into_umbrella(&self.fn_return_type, &node.expr) {
            return;
        }
        if expr_contains_map_err(&node.expr) {
            return;
        }
        if let Some((foreign_type, source_snippet)) = foreign_try_site(&node.expr) {
            self.push(
                ErrorChainProbeId::PreservedQuestionMark001,
                node.span().start().line as u32,
                format!("{source_snippet}?"),
                Some(foreign_type),
                ctx,
            );
        }
    }

    pub(super) fn into_records(self) -> Vec<ErrorChainRecord> {
        self.chain
    }
}

fn chain_expr_snippet(expr: &Expr) -> String {
    truncate_snippet(&raw_expr_snippet(expr), 96)
}

fn return_type_label(output: &ReturnType) -> Option<String> {
    match output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(type_label(ty)),
    }
}

fn try_propagates_into_umbrella(fn_return_type: &Option<String>, try_expr: &Expr) -> bool {
    if is_option_try_expr(try_expr) {
        return false;
    }
    return_type_is_umbrella(fn_return_type)
}

fn return_type_is_umbrella(fn_return_type: &Option<String>) -> bool {
    let Some(return_type) = fn_return_type else {
        return false;
    };
    if return_type.starts_with("Option") || return_type.contains("Option<") {
        return false;
    }
    if is_foreign_result_return_type(return_type) {
        return false;
    }
    return_type.contains("Result")
}

fn is_option_try_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::MethodCall(call) if call.method == "ok")
}

fn is_foreign_result_return_type(return_type: &str) -> bool {
    if return_type.contains("io::Result") {
        return true;
    }
    for foreign in [
        "std::io::Error",
        "serde_json::Error",
        "syn::Error",
        "csv::Error",
        "cargo_metadata::Error",
    ] {
        if return_type.contains(foreign) {
            return true;
        }
    }
    false
}

fn foreign_try_site(expr: &Expr) -> Option<(String, String)> {
    if expr_contains_map_err(expr) {
        return None;
    }
    let source_snippet = chain_expr_snippet(expr);
    let (foreign_type, _, _) = infer_foreign_error_type(&source_snippet)?;
    Some((foreign_type, source_snippet))
}

fn preserved_map_err_conversion(
    call: &ExprMethodCall,
    receiver: &Expr,
) -> Option<(String, String)> {
    let converter = call.args.first()?;
    if map_err_stringifies(converter) {
        return None;
    }
    if !map_err_preserves_chain(converter) {
        return None;
    }
    let source_snippet = chain_expr_snippet(receiver);
    let (foreign_type, _, _) = infer_foreign_error_type(&source_snippet)?;
    Some((foreign_type, source_snippet))
}

fn extract_from_source_type(trait_path: &syn::Path) -> Option<&Type> {
    let segment = trait_path.segments.last()?;
    if segment.ident != "From" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(from_type) = args.args.first()? else {
        return None;
    };
    Some(from_type)
}

fn map_err_stringifies(expr: &Expr) -> bool {
    chain_expr_contains_to_string(expr)
}

fn map_err_preserves_chain(expr: &Expr) -> bool {
    match expr {
        // `map_err(CrateError::from)` and `map_err(CrateError::cargo_metadata)` are
        // the preferred 1-arg wrap: a constructor that keeps the foreign error.
        Expr::Path(path) => {
            path_is_into_or_from(path) || !path_constructor_drops_typed_source(path)
        }
        Expr::Call(call) => path_is_into_or_from_fn(&call.func),
        Expr::Closure(closure) => closure_preserves_chain(closure),
        _ => false,
    }
}

fn path_is_into_or_from(path: &ExprPath) -> bool {
    let segments = &path.path.segments;
    if segments.len() == 2 && segments[0].ident == "Into" && segments[1].ident == "into" {
        return true;
    }
    segments
        .last()
        .is_some_and(|segment| segment.ident == "from")
}

fn path_is_into_or_from_fn(func: &Expr) -> bool {
    match func {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "from"),
        _ => false,
    }
}

fn closure_preserves_chain(closure: &ExprClosure) -> bool {
    if chain_expr_contains_to_string(&closure.body) {
        return false;
    }
    expr_contains_source_field(&closure.body) || expr_forwards_error_binding(&closure.body)
}

fn expr_forwards_error_binding(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) if path_is_into_or_from_fn(&call.func) => true,
        Expr::Call(call)
            if !constructor_drops_typed_source(&call.func)
                && call.args.iter().any(expr_is_error_binding) =>
        {
            true
        }
        Expr::Path(path) => path_is_into_or_from(path),
        Expr::Block(block) => block.block.stmts.last().is_some_and(|stmt| match stmt {
            syn::Stmt::Expr(inner, _) => expr_forwards_error_binding(inner),
            _ => false,
        }),
        Expr::Paren(paren) => expr_forwards_error_binding(&paren.expr),
        Expr::Group(group) => expr_forwards_error_binding(&group.expr),
        _ => false,
    }
}

fn constructor_drops_typed_source(func: &Expr) -> bool {
    let Expr::Path(path) = func else {
        return false;
    };
    path_constructor_drops_typed_source(path)
}

fn path_constructor_drops_typed_source(path: &ExprPath) -> bool {
    path.path.segments.last().is_some_and(|segment| {
        matches!(
            segment.ident.to_string().as_str(),
            "invariant" | "to_string" | "from_str"
        )
    })
}

fn expr_is_error_binding(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| ident == "e" || ident == "err" || ident == "error"),
        Expr::Paren(paren) => expr_is_error_binding(&paren.expr),
        _ => false,
    }
}

fn chain_expr_contains_to_string(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(call) if call.method == "to_string" => {
            expr_is_error_binding(&call.receiver)
        }
        Expr::Call(call) => {
            chain_expr_contains_to_string(&call.func)
                || call.args.iter().any(chain_expr_contains_to_string)
        }
        Expr::MethodCall(call) => {
            chain_expr_contains_to_string(&call.receiver)
                || call.args.iter().any(chain_expr_contains_to_string)
        }
        Expr::Closure(closure) => chain_expr_contains_to_string(&closure.body),
        Expr::Struct(item) => item
            .fields
            .iter()
            .any(|field| chain_expr_contains_to_string(&field.expr)),
        Expr::Field(field) => chain_expr_contains_to_string(&field.base),
        Expr::Paren(paren) => chain_expr_contains_to_string(&paren.expr),
        Expr::Group(group) => chain_expr_contains_to_string(&group.expr),
        Expr::Block(block) => block.block.stmts.iter().any(|stmt| match stmt {
            syn::Stmt::Expr(inner, _) => chain_expr_contains_to_string(inner),
            _ => false,
        }),
        _ => false,
    }
}

fn expr_contains_source_field(expr: &Expr) -> bool {
    match expr {
        Expr::Struct(item) => item.fields.iter().any(|field| {
            matches!(
                &field.member,
                Member::Named(ident) if ident == "source" || ident == "err"
            )
        }),
        Expr::Call(call) => {
            expr_contains_source_field(&call.func)
                || call.args.iter().any(expr_contains_source_field)
        }
        Expr::MethodCall(call) => {
            expr_contains_source_field(&call.receiver)
                || call.args.iter().any(expr_contains_source_field)
        }
        Expr::Closure(closure) => expr_contains_source_field(&closure.body),
        Expr::Field(field) => expr_contains_source_field(&field.base),
        Expr::Paren(paren) => expr_contains_source_field(&paren.expr),
        Expr::Group(group) => expr_contains_source_field(&group.expr),
        _ => false,
    }
}

fn expr_contains_map_err(expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(call) if call.method == "map_err" => true,
        Expr::Try(try_expr) => expr_contains_map_err(&try_expr.expr),
        Expr::Call(call) => {
            expr_contains_map_err(&call.func) || call.args.iter().any(expr_contains_map_err)
        }
        Expr::MethodCall(call) => {
            expr_contains_map_err(&call.receiver) || call.args.iter().any(expr_contains_map_err)
        }
        Expr::Field(field) => expr_contains_map_err(&field.base),
        Expr::Paren(paren) => expr_contains_map_err(&paren.expr),
        Expr::Group(group) => expr_contains_map_err(&group.expr),
        _ => false,
    }
}

fn is_foreign_rust_type(ty: &Type) -> bool {
    let label = type_label(ty);
    [
        "std::",
        "serde_json::",
        "serde_yaml::",
        "syn::",
        "csv::",
        "cargo_metadata::",
        "reqwest::",
        "url::",
        "toml::",
        "walkdir::",
        "tempfile::",
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || label.ends_with("Error") && label.contains("::") && !label.ends_with("ErrorKind")
}

fn enum_name_suggests_error_kind(name: &str) -> bool {
    name.ends_with("ErrorKind") || name.ends_with("Kind")
}

fn foreign_type_from_rust_type(ty: &Type) -> Option<String> {
    let label = type_label(ty);
    if label == "String" || label == "?" {
        return None;
    }
    Some(label)
}

fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn is_string_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.is_ident("String"),
        Type::Reference(reference) => is_string_type(&reference.elem),
        Type::Paren(paren) => is_string_type(&paren.elem),
        Type::Group(group) => is_string_type(&group.elem),
        _ => false,
    }
}

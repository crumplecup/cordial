//! Error-chain layer: preserved vs. discarded foreign error chains.
//!
//! Gated as a whole unit by `#[cfg(feature = "error_chain")]` on the `mod
//! chain_layer;` declaration in `error_ir/mod.rs` — nothing inside this
//! directory needs its own `#[cfg]`, since the entire module only compiles
//! when the feature is enabled.

mod preds;

use syn::spanned::Spanned;
use syn::{ExprMethodCall, ExprTry, Fields, ItemEnum, ItemImpl, ItemStruct, ReturnType};

use super::visitor::SiteCtx;
use crate::etiquettes::error_chain::{ErrorChainProbeId, ErrorChainRecord};

use self::preds::{
    enum_name_suggests_error_kind, expr_contains_map_err, extract_from_source_type,
    foreign_try_site, foreign_type_from_rust_type, is_foreign_rust_type, is_string_type,
    preserved_map_err_conversion, return_type_is_umbrella, return_type_label,
    try_propagates_into_umbrella, type_label,
};

use tracing::instrument;

/// Per-file accumulator for the `error_chain` layer.
#[derive(Default)]
pub(super) struct ChainLayer {
    fn_return_type: Option<String>,
    chain: Vec<ErrorChainRecord>,
}

impl ChainLayer {
    #[instrument(level = "debug")]
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Enter a fn/method body, returning the previous return-type label so
    /// the caller can restore it via [`Self::exit_fn`].
    #[instrument(level = "debug", skip(self, output))]
    pub(super) fn enter_fn(&mut self, output: &ReturnType) -> Option<String> {
        let prev = self.fn_return_type.take();
        self.fn_return_type = return_type_label(output);
        prev
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn exit_fn(&mut self, prev: Option<String>) {
        self.fn_return_type = prev;
    }

    #[instrument(level = "debug", skip(self, rule_id, ctx))]
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

    #[instrument(level = "debug", skip(self, item_struct, ctx))]
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

    #[instrument(level = "debug", skip(self, item_enum, ctx))]
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

    #[instrument(level = "debug", skip(self, item_impl, ctx))]
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

    #[instrument(level = "debug", skip(self, call, ctx))]
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

    #[instrument(level = "debug", skip(self, node, ctx))]
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

    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_records(self) -> Vec<ErrorChainRecord> {
        self.chain
    }
}

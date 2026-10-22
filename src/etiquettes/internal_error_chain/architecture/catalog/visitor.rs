use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, ExprCall, ExprMethodCall, Fields, FnArg, GenericArgument, ImplItem, ItemEnum,
    ItemFn, ItemImpl, ItemMod, ItemStruct, Meta, PathArguments, Token, Type,
    punctuated::Punctuated,
};
use tracing::instrument;

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;

use super::fanout::{
    call_name, direct_source_target, direct_target, literal_string, wrapper_target,
};
use super::{
    Catalog, ConstructorRec, EnumInfo, ErrorField, FanoutCall, FanoutWrapper, MapErrSite,
    StructInfo, VariantInfo,
};
use crate::etiquettes::internal_error_chain::source_shape::{
    block_captures_location, has_track_caller, returns_self, sig_takes_location_arg,
    type_is_location,
};
use crate::etiquettes::internal_error_chain::type_graph::{
    is_foreign_type_label, item_derives_error, trait_is_std_error, type_label,
};

#[instrument(level = "debug", skip(attrs))]
fn item_derives_copy(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            found |= meta.path.is_ident("Copy");
            Ok(())
        });
        found
    })
}

#[derive(derive_builder::Builder, derive_setters::Setters)]
#[builder(
    name = "CatalogVisitorBuilder",
    pattern = "owned",
    build_fn(name = "build_inner", private, error = "crate::error::CordialError")
)]
#[setters(generate = false, borrow_self)]
pub(in crate::etiquettes::internal_error_chain::architecture) struct CatalogVisitor<'a> {
    file: PathBuf,
    module_prefix: Vec<String>,
    catalog: &'a mut Catalog,
    #[builder(default)]
    inside_kind_call: bool,
}

impl<'a> CatalogVisitor<'a> {
    #[instrument(level = "debug")]
    pub(in crate::etiquettes::internal_error_chain::architecture) fn builder()
    -> CatalogVisitorBuilder<'a> {
        CatalogVisitorBuilder::default()
    }
}

impl<'a> CatalogVisitorBuilder<'a> {
    #[instrument(level = "debug", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::architecture) fn build(
        self,
    ) -> CordialResult<CatalogVisitor<'a>> {
        self.build_inner()
    }
}

impl CatalogVisitor<'_> {
    #[instrument(level = "debug", skip(self, fields))]
    fn record_fields(&mut self, owner: &str, variant: Option<&str>, fields: &Fields) {
        for (index, field) in fields.iter().enumerate() {
            let name = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("_{index}"));
            let mut context = self.qualified(owner);
            if let Some(variant) = variant {
                context.push_str("::");
                context.push_str(variant);
            }
            context.push_str("::");
            context.push_str(&name);
            self.catalog.fields.push(ErrorField {
                owner: owner.to_string(),
                name,
                context,
                file: self.file.clone(),
                line: field.ty.span().start().line as u32,
                ty: field.ty.clone(),
            });
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn qualified(&self, name: &str) -> String {
        if self.module_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    fn record_struct(&mut self, item: &ItemStruct) {
        if is_cfg_test(&item.attrs) {
            return;
        }
        let ident = item.ident.to_string();
        if item_derives_error(&item.attrs) {
            self.catalog.error_impls.insert(ident.clone());
        }
        if item_derives_copy(&item.attrs) {
            self.catalog.copy_impls.insert(ident.clone());
        }
        self.record_fields(&ident, None, &item.fields);
        let mut info = StructInfo {
            ident: ident.clone(),
            type_path: self.qualified(&ident),
            file: self.file.clone(),
            line: item.span().start().line as u32,
            snippet: format!("struct {ident}"),
            kind_box_of: None,
            kind_borrowed_of: None,
            kind_unboxed_of: None,
            foreign_source: None,
            has_source_field: false,
            has_file: false,
            has_line: false,
            has_location: false,
        };
        match &item.fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    let Some(name) = field.ident.as_ref().map(ToString::to_string) else {
                        continue;
                    };
                    if type_is_location(&field.ty) {
                        info.has_location = true;
                    }
                    match name.as_str() {
                        "kind" => {
                            if let Type::Reference(reference) = &field.ty {
                                let label = type_label(&reference.elem);
                                let last = Catalog::last_ident(&label).to_string();
                                if last.ends_with("Kind") {
                                    info.kind_borrowed_of = Some(last);
                                }
                            } else if let Some(inner) = box_inner(&field.ty) {
                                info.kind_box_of = Some(Catalog::last_ident(&inner).to_string());
                            } else {
                                let label = type_label(&field.ty);
                                let last = Catalog::last_ident(&label).to_string();
                                if last.ends_with("Kind") {
                                    info.kind_unboxed_of = Some(last);
                                }
                            }
                        }
                        "source" => {
                            info.has_source_field = true;
                            let label = type_label(&field.ty);
                            if is_foreign_type_label(&label) {
                                info.foreign_source = Some(label);
                            }
                        }
                        "file" => info.has_file = true,
                        "line" => info.has_line = true,
                        _ => {}
                    }
                }
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let label = type_label(&fields.unnamed[0].ty);
                if is_foreign_type_label(&label) {
                    info.snippet = format!("struct {ident}({label})");
                    info.foreign_source = Some(label);
                }
            }
            _ => {}
        }
        self.catalog.structs.insert(ident, info);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn record_enum(&mut self, item: &ItemEnum) {
        if is_cfg_test(&item.attrs) {
            return;
        }
        let ident = item.ident.to_string();
        if item_derives_error(&item.attrs) {
            self.catalog.error_impls.insert(ident.clone());
        }
        if item_derives_copy(&item.attrs) {
            self.catalog.copy_impls.insert(ident.clone());
        }
        let mut variants = Vec::new();
        for variant in &item.variants {
            self.record_fields(&ident, Some(&variant.ident.to_string()), &variant.fields);
            let payloads = variant_payloads(&variant.fields);
            variants.push(VariantInfo {
                name: variant.ident.to_string(),
                line: variant.span().start().line as u32,
                snippet: format!("{ident}::{}", variant.ident),
                payloads,
            });
        }
        self.catalog.enums.insert(
            ident.clone(),
            EnumInfo {
                ident: ident.clone(),
                type_path: self.qualified(&ident),
                file: self.file.clone(),
                line: item.span().start().line as u32,
                snippet: format!("enum {ident}"),
                variants,
            },
        );
    }

    #[instrument(level = "debug", skip(self, item))]
    fn record_impl(&mut self, item: &ItemImpl) {
        if is_cfg_test(&item.attrs) {
            return;
        }
        let self_ident = Catalog::last_ident(&type_label(&item.self_ty)).to_string();
        if item
            .trait_
            .as_ref()
            .is_some_and(|(path, _)| path.segments.last().is_some_and(|seg| seg.ident == "Copy"))
        {
            self.catalog.copy_impls.insert(self_ident.clone());
        }
        if let Some((trait_path, _)) = &item.trait_
            && trait_is_std_error(trait_path)
        {
            self.catalog.error_impls.insert(self_ident);
            return;
        }
        if let Some((trait_path, _)) = &item.trait_ {
            let Some(seg) = trait_path.segments.last() else {
                return;
            };
            if seg.ident != "From" {
                return;
            }
            for impl_item in &item.items {
                let ImplItem::Fn(method) = impl_item else {
                    continue;
                };
                if method.sig.ident != "from" {
                    continue;
                }
                self.catalog.constructors.push(ConstructorRec {
                    self_ident: self_ident.clone(),
                    name: "From::from".to_string(),
                    line: method.span().start().line as u32,
                    has_track_caller: has_track_caller(&method.attrs),
                    instrument_ret: has_instrument_ret(&method.attrs),
                    captures_location: block_captures_location(&method.block),
                    from_trait: true,
                    input_labels: input_type_labels(&method.sig),
                    takes_location_arg: sig_takes_location_arg(&method.sig),
                });
            }
            return;
        }
        for impl_item in &item.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            if method
                .sig
                .inputs
                .iter()
                .any(|arg| matches!(arg, FnArg::Receiver(_)))
            {
                continue;
            }
            if !returns_self(&method.sig, &self_ident) {
                continue;
            }
            self.catalog.constructors.push(ConstructorRec {
                self_ident: self_ident.clone(),
                name: method.sig.ident.to_string(),
                line: method.span().start().line as u32,
                has_track_caller: has_track_caller(&method.attrs),
                instrument_ret: has_instrument_ret(&method.attrs),
                captures_location: block_captures_location(&method.block),
                from_trait: false,
                input_labels: input_type_labels(&method.sig),
                takes_location_arg: sig_takes_location_arg(&method.sig),
            });
        }
    }
}

impl<'ast> Visit<'ast> for CatalogVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        if let Some(target) = wrapper_target(&node.sig, &node.block) {
            self.catalog.fanout_wrappers.push(FanoutWrapper::new(
                self.file.clone(),
                self.module_prefix.clone(),
                node.sig.ident.to_string(),
                node.sig.ident.span().start().line as u32,
                target,
            ));
        }
        syn::visit::visit_item_fn(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        let direct = direct_target(node);
        if let Some((target, literal)) = &direct {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                self.module_prefix.clone(),
                node.span().start().line as u32,
                String::new(),
                Some(target.clone()),
                literal.clone(),
            ));
        } else if !self.inside_kind_call
            && let Some((target, literal)) = direct_source_target(node)
        {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                self.module_prefix.clone(),
                node.span().start().line as u32,
                String::new(),
                Some(target),
                literal,
            ));
        } else if node.args.len() == 1
            && let (Some(callee), Some(literal)) = (
                call_name(&node.func),
                node.args.first().and_then(literal_string),
            )
        {
            self.catalog.fanout_calls.push(FanoutCall::new(
                self.file.clone(),
                self.module_prefix.clone(),
                node.span().start().line as u32,
                callee,
                None,
                literal,
            ));
        }
        let was_inside_kind_call = self.inside_kind_call;
        self.inside_kind_call |= direct.is_some();
        syn::visit::visit_expr_call(self, node);
        self.inside_kind_call = was_inside_kind_call;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if node.method == "map_err"
            && let Some(Expr::Path(converter)) = node.args.first()
        {
            let segments: Vec<_> = converter.path.segments.iter().collect();
            if segments.len() >= 2 {
                let method = segments[segments.len() - 1].ident.to_string();
                let type_ident = segments[segments.len() - 2].ident.to_string();
                self.catalog.map_err_sites.push(MapErrSite {
                    type_ident,
                    method,
                    file: self.file.clone(),
                    line: node.method.span().start().line as u32,
                });
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let Some((_, items)) = &node.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(node.ident.to_string());
        let prev = self.module_prefix.clone();
        self.module_prefix = nested;
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.record_struct(node);
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.record_enum(node);
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.record_impl(node);
        syn::visit::visit_item_impl(self, node);
    }
}

#[instrument(level = "trace", skip(attrs), ret)]
fn has_instrument_ret(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "instrument")
        })
        .any(|attr| {
            attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .is_ok_and(|args| args.iter().any(|arg| arg.path().is_ident("ret")))
        })
}

#[instrument(level = "debug", skip(ty))]
fn box_inner(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference(_) => None,
        Type::Paren(paren) => box_inner(&paren.elem),
        Type::Group(group) => box_inner(&group.elem),
        Type::Path(path) => {
            let last = path.path.segments.last()?;
            if last.ident != "Box" {
                return None;
            }
            let PathArguments::AngleBracketed(args) = &last.arguments else {
                return None;
            };
            args.args.iter().find_map(|arg| match arg {
                GenericArgument::Type(inner) => Some(type_label(inner)),
                _ => None,
            })
        }
        _ => None,
    }
}

#[instrument(level = "debug", skip(sig))]
fn input_type_labels(sig: &syn::Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat) => Some(type_label(&pat.ty)),
            FnArg::Receiver(_) => None,
        })
        .collect()
}

#[instrument(level = "debug", skip(fields))]
fn variant_payloads(fields: &Fields) -> Vec<String> {
    match fields {
        Fields::Unit => Vec::new(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .map(|field| type_label(&field.ty))
            .collect(),
        Fields::Named(named) => named
            .named
            .iter()
            .map(|field| type_label(&field.ty))
            .collect(),
    }
}

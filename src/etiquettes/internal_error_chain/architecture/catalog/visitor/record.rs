//! Facts recorded once a struct, enum, or impl item has been visited --
//! split out of `visitor.rs` (the AST walk itself) once that file grew
//! past a human reader's single-take size.

use syn::spanned::Spanned;
use syn::{
    Attribute, Fields, FnArg, GenericArgument, ImplItem, ItemEnum, ItemImpl, ItemStruct, Meta,
    PathArguments, Token, Type, punctuated::Punctuated,
};
use tracing::instrument;

use crate::enricher::is_cfg_test;

use super::super::fanout::wrapper_target;
use super::super::{
    Catalog, ConstructorRec, EnumInfo, ErrorField, FanoutWrapper, StructInfo, VariantInfo,
};
use super::CatalogVisitor;
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

impl CatalogVisitor<'_> {
    #[instrument(level = "debug", skip(self, fields))]
    pub(super) fn record_fields(&mut self, owner: &str, variant: Option<&str>, fields: &Fields) {
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
    pub(super) fn qualified(&self, name: &str) -> String {
        if self.module_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, item))]
    pub(super) fn record_struct(&mut self, item: &ItemStruct) {
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
    pub(super) fn record_enum(&mut self, item: &ItemEnum) {
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
    pub(super) fn record_impl(&mut self, item: &ItemImpl) {
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
            // Checked before the constructor-only filters below:
            // `AmenableError::io`/`::invariant` and every wrapper
            // constructor like them live inside a plain `impl` block,
            // never as a bare top-level `fn` -- `visit_item_fn` alone
            // never sees them, so this was the one place a wrapper
            // actually used by the whole crate could be registered.
            if let Some(target) = wrapper_target(&method.sig, &method.block) {
                self.catalog.fanout_wrappers.push(FanoutWrapper::new(
                    self.file.clone(),
                    Some(self_ident.clone()),
                    method.sig.ident.to_string(),
                    method.sig.ident.span().start().line as u32,
                    target,
                ));
            }
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

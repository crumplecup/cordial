//! Collected Error-implementing types for architecture lints.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, GenericArgument, ImplItem, ItemEnum, ItemImpl, ItemMod, ItemStruct,
    PathArguments, Type,
};

use crate::enricher::is_cfg_test;

use super::super::source_shape::{
    block_captures_location, has_track_caller, returns_self, sig_takes_location_arg,
    type_is_location,
};
use super::super::type_graph::{
    is_foreign_type_label, item_derives_error, last_ident, trait_is_std_error, type_label,
};

use tracing::instrument;
pub(super) enum CatalogPhase {
    Types,
    Impls,
}

pub(super) struct StructInfo {
    pub(super) ident: String,
    pub(super) type_path: String,
    pub(super) file: PathBuf,
    pub(super) line: u32,
    pub(super) snippet: String,
    pub(super) kind_box_of: Option<String>,
    pub(super) kind_unboxed_of: Option<String>,
    pub(super) foreign_source: Option<String>,
    pub(super) has_source_field: bool,
    pub(super) has_file: bool,
    pub(super) has_line: bool,
    pub(super) has_location: bool,
}

impl StructInfo {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn location_complete(&self) -> bool {
        (self.has_file && self.has_line) || self.has_location
    }
}

pub(super) struct VariantInfo {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) snippet: String,
    pub(super) payloads: Vec<String>,
}

pub(super) struct EnumInfo {
    pub(super) ident: String,
    pub(super) type_path: String,
    pub(super) file: PathBuf,
    pub(super) line: u32,
    pub(super) snippet: String,
    pub(super) variants: Vec<VariantInfo>,
}

pub(super) struct ConstructorRec {
    pub(super) self_ident: String,
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) has_track_caller: bool,
    pub(super) captures_location: bool,
    pub(super) from_trait: bool,
    pub(super) input_labels: Vec<String>,
    pub(super) takes_location_arg: bool,
}

pub(super) struct Catalog {
    pub(super) crate_name: String,
    pub(super) structs: BTreeMap<String, StructInfo>,
    pub(super) enums: BTreeMap<String, EnumInfo>,
    pub(super) constructors: Vec<ConstructorRec>,
    pub(super) error_impls: BTreeSet<String>,
}

impl Catalog {
    #[instrument(level = "debug", fields(crate_name = crate_name))]
    pub(super) fn new(crate_name: &str) -> Self {
        Self {
            crate_name: crate_name.to_string(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            constructors: Vec::new(),
            error_impls: BTreeSet::new(),
        }
    }

    #[instrument(level = "debug")]
    pub(super) fn last_ident(label: &str) -> &str {
        last_ident(label)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn impls_error(&self, ident: &str) -> bool {
        self.error_impls.contains(ident)
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_kind_name(ident: &str) -> bool {
        ident.ends_with("Kind")
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_error_enum_name(ident: &str) -> bool {
        ident.ends_with("Error") && !Self::is_kind_name(ident)
    }

    #[instrument(level = "trace", skip(self, item))]
    pub(super) fn is_error_kind(&self, item: &EnumInfo) -> bool {
        let boxed_by_error = self.structs.values().any(|item_struct| {
            self.impls_error(&item_struct.ident)
                && (item_struct.kind_box_of.as_deref() == Some(item.ident.as_str())
                    || item_struct.kind_unboxed_of.as_deref() == Some(item.ident.as_str()))
        });
        if boxed_by_error {
            return true;
        }
        Self::is_kind_name(&item.ident)
            && item.variants.iter().any(|variant| {
                variant
                    .payloads
                    .iter()
                    .any(|payload| self.impls_error(Self::last_ident(payload)))
            })
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn kind_payload_idents(&self) -> BTreeSet<String> {
        let mut idents = BTreeSet::new();
        for item in self.enums.values() {
            if !self.is_error_kind(item) {
                continue;
            }
            for variant in &item.variants {
                for payload in &variant.payloads {
                    idents.insert(Self::last_ident(payload).to_string());
                }
            }
        }
        idents
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn root_parents(&self) -> Vec<&StructInfo> {
        let payloads = self.kind_payload_idents();
        self.structs
            .values()
            .filter(|item| {
                self.impls_error(&item.ident)
                    && item.kind_box_of.is_some()
                    && !payloads.contains(&item.ident)
            })
            .collect()
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn native_source_idents(&self) -> BTreeSet<String> {
        let parents: BTreeSet<String> = self
            .root_parents()
            .into_iter()
            .map(|item| item.ident.clone())
            .collect();
        self.error_impls
            .iter()
            .filter(|ident| self.structs.contains_key(*ident) && !parents.contains(*ident))
            .cloned()
            .collect()
    }
}

pub(super) struct CatalogVisitor<'a> {
    pub(super) file: PathBuf,
    pub(super) module_prefix: Vec<String>,
    pub(super) catalog: &'a mut Catalog,
    pub(super) phase: CatalogPhase,
}

impl CatalogVisitor<'_> {
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
        let mut info = StructInfo {
            ident: ident.clone(),
            type_path: self.qualified(&ident),
            file: self.file.clone(),
            line: item.span().start().line as u32,
            snippet: format!("struct {ident}"),
            kind_box_of: None,
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
                    match name.as_str() {
                        "kind" => {
                            if let Some(inner) = box_inner(&field.ty) {
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
                        "location" if type_is_location(&field.ty) => info.has_location = true,
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
        let mut variants = Vec::new();
        for variant in &item.variants {
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
        if let Some((_, trait_path, _)) = &item.trait_
            && trait_is_std_error(trait_path)
        {
            self.catalog.error_impls.insert(self_ident);
            return;
        }
        if !self.catalog.structs.contains_key(&self_ident) {
            return;
        }
        if let Some((_, trait_path, _)) = &item.trait_ {
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
        if matches!(self.phase, CatalogPhase::Types) {
            self.record_struct(node);
        }
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if matches!(self.phase, CatalogPhase::Types) {
            self.record_enum(node);
        }
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if matches!(self.phase, CatalogPhase::Impls) {
            self.record_impl(node);
        }
        syn::visit::visit_item_impl(self, node);
    }
}

#[instrument(level = "debug", skip(ty))]
fn box_inner(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference(reference) => box_inner(&reference.elem),
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

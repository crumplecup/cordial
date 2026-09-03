//! One-file syn visitor that records Error-implementing types.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Fields, ItemEnum, ItemImpl, ItemMod, ItemStruct};

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::super::types::InternalErrorTypeProbeId;
use super::walk::{
    extract_source_return_type, is_string_type, item_derives_error, last_ident,
    qualified_type_name, trait_is_std_error, type_label,
};

use tracing::instrument;

/// Raw type-graph facts plus the `Error`-implementing type idents in one file.
#[derive(Debug, Default, derive_new::new, derive_getters::Getters)]
pub(crate) struct RawTypeGraphScan {
    nodes: Vec<RawTypeNode>,
    error_impls: BTreeSet<String>,
}

/// Collect raw type-graph nodes from a pre-parsed source file.
#[instrument(level = "debug", skip(syntax, file))]
pub(crate) fn scan_error_rust_syntax_raw(
    syntax: &syn::File,
    file: &Path,
    module_root: &Path,
) -> CordialResult<RawTypeGraphScan> {
    let module_prefix = module_path_from_src_file(module_root, file);
    let mut visitor = TypeGraphScanVisitor {
        file: file.to_path_buf(),
        module_prefix,
        raw_nodes: Vec::new(),
        error_impls: BTreeSet::new(),
        error: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(RawTypeGraphScan::new(
        visitor.raw_nodes,
        visitor.error_impls,
    ))
}

#[instrument(level = "debug", skip(file), err(level = "warn"))]
pub(super) fn scan_error_rust_file_raw(
    file: &Path,
    module_root: &Path,
) -> CordialResult<RawTypeGraphScan> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    scan_error_rust_syntax_raw(&syntax, file, module_root)
}

#[derive(Debug, Clone, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub(crate) struct RawTypeNode {
    type_path: String,
    #[getter(copy)]
    probe_id: InternalErrorTypeProbeId,
    source_target: Option<String>,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
}

impl RawTypeNode {
    /// Start a builder for this value.
    pub fn builder() -> RawTypeNodeBuilder {
        RawTypeNodeBuilder::default()
    }
}

struct TypeGraphScanVisitor {
    file: PathBuf,
    module_prefix: Vec<String>,
    raw_nodes: Vec<RawTypeNode>,
    error_impls: BTreeSet<String>,
    error: Option<crate::error::CordialError>,
}

impl TypeGraphScanVisitor {
    #[instrument(level = "debug", skip(self))]
    fn push_node(
        &mut self,
        type_path: String,
        probe_id: InternalErrorTypeProbeId,
        source_target: Option<String>,
        line: u32,
        snippet: String,
    ) {
        if self.error.is_some() {
            return;
        }
        match RawTypeNode::builder()
            .type_path(type_path)
            .probe_id(probe_id)
            .source_target(source_target)
            .file(self.file.clone())
            .line(line)
            .snippet(snippet)
            .build()
        {
            Ok(node) => self.raw_nodes.push(node),
            Err(error) => self.error = Some(error),
        }
    }

    #[instrument(level = "debug", skip(self, item_struct))]
    fn check_wrapper_struct(&mut self, item_struct: &ItemStruct) {
        let type_path = qualified_type_name(&self.module_prefix, &item_struct.ident.to_string());
        if item_struct.ident == "CordialError" {
            self.push_node(
                type_path.clone(),
                InternalErrorTypeProbeId::InternalLink001,
                Some("CordialErrorKind".to_string()),
                item_struct.span().start().line as u32,
                format!(
                    "struct {} {{ kind: CordialErrorKind, … }}",
                    item_struct.ident
                ),
            );
            return;
        }

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
            let target = type_label(&field.ty);
            self.push_node(
                type_path.clone(),
                InternalErrorTypeProbeId::InternalLink001,
                Some(target.clone()),
                field.span().start().line as u32,
                format!("struct {} {{ source: {} }}", item_struct.ident, target),
            );
        }
    }

    #[instrument(level = "debug", skip(self, item_enum))]
    fn check_error_enum(&mut self, item_enum: &ItemEnum) {
        let enum_name = item_enum.ident.to_string();
        for variant in &item_enum.variants {
            let variant_path = format!("{enum_name}::{}", variant.ident);
            match &variant.fields {
                Fields::Named(fields) => {
                    let mut has_source = false;
                    let mut has_string_detail = false;
                    let mut source_target = None;
                    for field in &fields.named {
                        let Some(ident) = &field.ident else {
                            continue;
                        };
                        if ident == "source" || ident == "err" {
                            has_source = true;
                            if !is_string_type(&field.ty) {
                                source_target = Some(type_label(&field.ty));
                            }
                        }
                        if (ident == "detail" || ident == "path" || ident == "message")
                            && is_string_type(&field.ty)
                        {
                            has_string_detail = true;
                        }
                    }
                    if has_string_detail && !has_source {
                        self.push_node(
                            variant_path.clone(),
                            InternalErrorTypeProbeId::InternalLeaf001,
                            None,
                            variant.span().start().line as u32,
                            format!("enum {enum_name} {{ {variant_path} {{ … }} }}"),
                        );
                    } else if let Some(target) = source_target {
                        self.push_node(
                            variant_path,
                            InternalErrorTypeProbeId::InternalLink001,
                            Some(target),
                            variant.span().start().line as u32,
                            format!("enum {enum_name} {{ {} {{ source: … }} }}", variant.ident),
                        );
                    }
                }
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let payload = &fields.unnamed[0].ty;
                    if is_string_type(payload) {
                        self.push_node(
                            variant_path.clone(),
                            InternalErrorTypeProbeId::InternalLeaf001,
                            None,
                            variant.span().start().line as u32,
                            format!("enum {enum_name} {{ {variant_path}(String) }}"),
                        );
                    } else {
                        self.push_node(
                            variant_path,
                            InternalErrorTypeProbeId::InternalLink001,
                            Some(type_label(payload)),
                            variant.span().start().line as u32,
                            format!(
                                "enum {enum_name} {{ {}({}) }}",
                                variant.ident,
                                type_label(payload)
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn mark_error_ident(&mut self, ident: &str) {
        self.error_impls.insert(ident.to_string());
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn check_error_source_impl(&mut self, item_impl: &ItemImpl) {
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            return;
        };
        if !trait_is_std_error(trait_path) {
            return;
        }
        let self_type = type_label(&item_impl.self_ty);
        self.mark_error_ident(last_ident(&self_type));
        let Some(target) = extract_source_return_type(item_impl) else {
            return;
        };
        self.push_node(
            self_type.clone(),
            InternalErrorTypeProbeId::InternalNested001,
            Some(target),
            item_impl.span().start().line as u32,
            format!("impl Error for {self_type} {{ fn source() -> … }}"),
        );
    }
}

impl<'ast> Visit<'ast> for TypeGraphScanVisitor {
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
        if item_derives_error(&node.attrs) {
            self.mark_error_ident(&node.ident.to_string());
        }
        self.check_wrapper_struct(node);
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if item_derives_error(&node.attrs) {
            self.mark_error_ident(&node.ident.to_string());
        }
        self.check_error_enum(node);
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.check_error_source_impl(node);
        syn::visit::visit_item_impl(self, node);
    }
}

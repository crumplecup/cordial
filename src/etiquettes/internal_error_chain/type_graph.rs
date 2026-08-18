//! Static scan of crate error types under `src/error.rs` and `src/error/`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Fields, ItemEnum, ItemImpl, ItemMod, ItemStruct, Type, TypePath};
use walkdir::WalkDir;

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;

use super::types::{
    InternalErrorNodeClass, InternalErrorTypeGraphReport, InternalErrorTypeNode,
    InternalErrorTypeProbeId,
};

use tracing::instrument;
/// Scan `src/error.rs` and `src/error/**` for the internal error type graph.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_internal_error_type_graph(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<InternalErrorTypeGraphReport> {
    let src_root = crate_root.join("src");
    let error_dir = src_root.join("error");
    let error_file = src_root.join("error.rs");
    let mut raw_nodes = Vec::new();

    if error_file.is_file() {
        raw_nodes.extend(scan_error_rust_file_raw(&error_file, &src_root)?);
    }
    if error_dir.is_dir() {
        for entry in WalkDir::new(&error_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            raw_nodes.extend(scan_error_rust_file_raw(path, &error_dir)?);
        }
    }

    let mut nodes = finalize_type_graph(raw_nodes, crate_name);
    for node in &mut nodes {
        if let Ok(rel) = node.file.strip_prefix(crate_root) {
            node.file = rel.to_path_buf();
        }
    }

    Ok(InternalErrorTypeGraphReport {
        crate_name: crate_name.to_string(),
        nodes,
    })
}

/// Scan one error-module source file (used by tests).
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_error_rust_source(
    source: &str,
    file: &Path,
    error_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorTypeNode>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(finalize_type_graph(
        scan_error_rust_syntax_raw(&syntax, file, error_root),
        crate_name,
    ))
}

/// Collect raw type-graph nodes from a pre-parsed error-module file.
#[instrument(level = "debug", skip(syntax, file))]
pub(crate) fn scan_error_rust_syntax_raw(
    syntax: &syn::File,
    file: &Path,
    error_root: &Path,
) -> Vec<RawTypeNode> {
    let module_prefix = module_path_from_error_file(error_root, file);
    let mut visitor = TypeGraphScanVisitor {
        file: file.to_path_buf(),
        module_prefix,
        raw_nodes: Vec::new(),
    };
    visitor.visit_file(syntax);
    visitor.raw_nodes
}

fn scan_error_rust_file_raw(file: &Path, error_root: &Path) -> CordialResult<Vec<RawTypeNode>> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(scan_error_rust_syntax_raw(&syntax, file, error_root))
}

#[derive(Debug)]
pub(crate) struct RawTypeNode {
    type_path: String,
    probe_id: InternalErrorTypeProbeId,
    source_target: Option<String>,
    file: PathBuf,
    line: u32,
    snippet: String,
}

struct TypeGraphScanVisitor {
    file: PathBuf,
    module_prefix: Vec<String>,
    raw_nodes: Vec<RawTypeNode>,
}

impl TypeGraphScanVisitor {
    fn check_wrapper_struct(&mut self, item_struct: &ItemStruct) {
        let type_path = qualified_type_name(&self.module_prefix, &item_struct.ident.to_string());
        if item_struct.ident == "CordialError" {
            self.raw_nodes.push(RawTypeNode {
                type_path: type_path.clone(),
                probe_id: InternalErrorTypeProbeId::InternalLink001,
                source_target: Some("CordialErrorKind".to_string()),
                file: self.file.clone(),
                line: item_struct.span().start().line as u32,
                snippet: format!(
                    "struct {} {{ kind: CordialErrorKind, … }}",
                    item_struct.ident
                ),
            });
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
            self.raw_nodes.push(RawTypeNode {
                type_path: type_path.clone(),
                probe_id: InternalErrorTypeProbeId::InternalLink001,
                source_target: Some(target.clone()),
                file: self.file.clone(),
                line: field.span().start().line as u32,
                snippet: format!("struct {} {{ source: {} }}", item_struct.ident, target),
            });
        }
    }

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
                        self.raw_nodes.push(RawTypeNode {
                            type_path: variant_path.clone(),
                            probe_id: InternalErrorTypeProbeId::InternalLeaf001,
                            source_target: None,
                            file: self.file.clone(),
                            line: variant.span().start().line as u32,
                            snippet: format!("enum {enum_name} {{ {variant_path} {{ … }} }}"),
                        });
                    } else if let Some(target) = source_target {
                        self.raw_nodes.push(RawTypeNode {
                            type_path: variant_path,
                            probe_id: InternalErrorTypeProbeId::InternalLink001,
                            source_target: Some(target),
                            file: self.file.clone(),
                            line: variant.span().start().line as u32,
                            snippet: format!(
                                "enum {enum_name} {{ {} {{ source: … }} }}",
                                variant.ident
                            ),
                        });
                    }
                }
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let payload = &fields.unnamed[0].ty;
                    if is_string_type(payload) {
                        self.raw_nodes.push(RawTypeNode {
                            type_path: variant_path.clone(),
                            probe_id: InternalErrorTypeProbeId::InternalLeaf001,
                            source_target: None,
                            file: self.file.clone(),
                            line: variant.span().start().line as u32,
                            snippet: format!("enum {enum_name} {{ {variant_path}(String) }}"),
                        });
                    } else {
                        self.raw_nodes.push(RawTypeNode {
                            type_path: variant_path,
                            probe_id: InternalErrorTypeProbeId::InternalLink001,
                            source_target: Some(type_label(payload)),
                            file: self.file.clone(),
                            line: variant.span().start().line as u32,
                            snippet: format!(
                                "enum {enum_name} {{ {}({}) }}",
                                variant.ident,
                                type_label(payload)
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn check_error_source_impl(&mut self, item_impl: &ItemImpl) {
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            return;
        };
        let Some(trait_name) = trait_path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            return;
        };
        if trait_name != "Error" {
            return;
        }
        let self_type = type_label(&item_impl.self_ty);
        let Some(target) = extract_source_return_type(item_impl) else {
            return;
        };
        self.raw_nodes.push(RawTypeNode {
            type_path: self_type.clone(),
            probe_id: InternalErrorTypeProbeId::InternalNested001,
            source_target: Some(target),
            file: self.file.clone(),
            line: item_impl.span().start().line as u32,
            snippet: format!("impl Error for {self_type} {{ fn source() -> … }}"),
        });
    }
}

impl<'ast> Visit<'ast> for TypeGraphScanVisitor {
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

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.check_wrapper_struct(node);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.check_error_enum(node);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.check_error_source_impl(node);
        syn::visit::visit_item_impl(self, node);
    }
}

#[instrument(level = "debug")]
pub(crate) fn finalize_type_graph(
    raw_nodes: Vec<RawTypeNode>,
    crate_name: &str,
) -> Vec<InternalErrorTypeNode> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for raw in &raw_nodes {
        if let Some(target) = &raw.source_target {
            edges
                .entry(raw.type_path.clone())
                .or_default()
                .insert(target.clone());
        }
    }

    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for raw in raw_nodes {
        let node_class = classify_node(&raw);
        let (reaches_foreign, chain_depth) = graph_metrics(&raw.type_path, &edges);
        nodes.push(InternalErrorTypeNode {
            crate_name: crate_name.to_string(),
            type_path: raw.type_path,
            node_class,
            probe_id: raw.probe_id,
            source_target: raw.source_target,
            reaches_foreign,
            chain_depth,
            file: raw.file,
            line: raw.line,
            snippet: raw.snippet,
        });
    }

    nodes.sort_by(|a, b| a.type_path.cmp(&b.type_path).then(a.line.cmp(&b.line)));
    nodes
}

fn classify_node(raw: &RawTypeNode) -> InternalErrorNodeClass {
    if raw.type_path == "CordialError" {
        return InternalErrorNodeClass::UmbrellaWrapper;
    }
    if raw.probe_id == InternalErrorTypeProbeId::InternalLeaf001 {
        return InternalErrorNodeClass::InternalLeaf;
    }
    if let Some(target) = &raw.source_target {
        if raw.type_path.ends_with("Source") && is_foreign_type_label(target) {
            return InternalErrorNodeClass::ForeignBridge;
        }
        if is_foreign_type_label(target) {
            return InternalErrorNodeClass::ForeignBridge;
        }
        return InternalErrorNodeClass::InternalLink;
    }
    InternalErrorNodeClass::InternalLink
}

fn graph_metrics(start: &str, edges: &BTreeMap<String, BTreeSet<String>>) -> (bool, u32) {
    let mut visited = BTreeSet::new();
    let mut queue = vec![(start.to_string(), 0u32)];
    let mut reaches_foreign = false;
    let mut max_depth = 0u32;

    while let Some((node, depth)) = queue.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        max_depth = max_depth.max(depth);
        if is_foreign_type_label(&node) {
            reaches_foreign = true;
        }
        let Some(targets) = edges.get(&node) else {
            continue;
        };
        for target in targets {
            if is_foreign_type_label(target) {
                reaches_foreign = true;
            }
            queue.push((target.clone(), depth + 1));
        }
    }

    (reaches_foreign, max_depth)
}

fn extract_source_return_type(item_impl: &ItemImpl) -> Option<String> {
    for item in &item_impl.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        if method.sig.ident != "source" {
            continue;
        }
        let syn::Stmt::Expr(syn::Expr::Match(match_expr), _) = method.block.stmts.first()? else {
            continue;
        };
        for arm in &match_expr.arms {
            if let syn::Expr::Path(path) = &*arm.body {
                return Some(type_path_label(&path.path));
            }
        }
    }
    None
}

#[instrument(level = "debug", skip(file))]
pub(crate) fn module_path_from_error_file(error_root: &Path, file: &Path) -> Vec<String> {
    let Ok(rel) = file.strip_prefix(error_root) else {
        return Vec::new();
    };
    let rel = rel.with_extension("");
    rel.components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect()
}

fn qualified_type_name(module_prefix: &[String], name: &str) -> String {
    if module_prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", module_prefix.join("::"), name)
    }
}

fn is_foreign_type_label(label: &str) -> bool {
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
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || (label.ends_with("Error") && label.contains("::"))
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

fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn type_path_label(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

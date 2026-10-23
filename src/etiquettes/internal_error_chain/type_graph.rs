//! Static scan of crate error types: anything that implements `Error`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::CordialResult;

use super::types::{InternalErrorNodeClass, InternalErrorTypeNode, InternalErrorTypeProbeId};

use tracing::instrument;
mod visitor;
mod walk;

pub(crate) use visitor::{RawTypeNode, scan_error_rust_syntax_raw};
pub(crate) use walk::{
    for_each_shared_src_rust_file, is_foreign_type_label, item_derives_error, last_ident,
    trait_is_std_error, type_label, type_path_is_error_related,
};

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
    finalize_type_graph(
        scan_error_rust_syntax_raw(&syntax, file, error_root)?
            .nodes()
            .clone(),
        crate_name,
    )
}

#[instrument(level = "debug", skip(raw_nodes), err(level = "warn"))]
pub(crate) fn finalize_type_graph(
    raw_nodes: Vec<RawTypeNode>,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorTypeNode>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for raw in &raw_nodes {
        if let Some(target) = raw.source_target() {
            edges
                .entry(raw.type_path().clone())
                .or_default()
                .insert(target.clone());
        }
    }

    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for raw in raw_nodes {
        let node_class = classify_node(&raw);
        let (reaches_foreign, chain_depth) = graph_metrics(raw.type_path(), &edges);
        nodes.push(
            InternalErrorTypeNode::builder()
                .crate_name(crate_name.to_string())
                .type_path(raw.type_path().clone())
                .node_class(node_class)
                .probe_id(raw.probe_id())
                .source_target(raw.source_target().clone())
                .reaches_foreign(reaches_foreign)
                .chain_depth(chain_depth)
                .file(raw.file().clone())
                .line(raw.line())
                .snippet(raw.snippet().clone())
                .build()?,
        );
    }

    nodes.sort_by(|a, b| {
        a.type_path()
            .cmp(b.type_path())
            .then(a.line().cmp(&b.line()))
    });
    Ok(nodes)
}

#[instrument(level = "debug", skip(raw))]
fn classify_node(raw: &RawTypeNode) -> InternalErrorNodeClass {
    if raw.type_path() == "CordialError" {
        return InternalErrorNodeClass::UmbrellaWrapper;
    }
    if raw.probe_id() == InternalErrorTypeProbeId::InternalLeaf001 {
        return InternalErrorNodeClass::InternalLeaf;
    }
    if let Some(target) = raw.source_target() {
        if raw.type_path().ends_with("Source") && is_foreign_type_label(target) {
            return InternalErrorNodeClass::ForeignBridge;
        }
        if is_foreign_type_label(target) {
            return InternalErrorNodeClass::ForeignBridge;
        }
        return InternalErrorNodeClass::InternalLink;
    }
    InternalErrorNodeClass::InternalLink
}

#[instrument(level = "debug", skip(edges))]
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

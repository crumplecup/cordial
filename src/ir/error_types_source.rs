//! Snapshot-bound error type relationships collected by the shared loader.

use crate::etiquettes::internal_error_chain::{
    InternalErrorTypeGraphReport, InternalErrorTypeProbeId, finalize_type_graph,
    type_path_is_error_related,
};
use crate::ir::error_sites_source::native::excluded_module;
use crate::loader::{DatabaseSource, module_path_from_src_file, path_has_fixtures};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{
    AstNode, SyntaxNode, WalkEvent,
    ast::{self, HasName},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod declarations;
mod variants;
pub(crate) use declarations::derives_error;

/// Required error-type evidence on the shared crate graph root.
pub const ATTR_SOURCE_ERROR_TYPES: &str = "source_error_types";

/// Complete raw relationships and implementors, including files with no nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorTypes {
    crate_root: PathBuf,
    files: Vec<ErrorTypesFile>,
}

/// Type facts bound to the source snapshot that supplied them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorTypesFile {
    snapshot: FileSnapshot,
    nodes: Vec<ErrorTypeSourceNode>,
    error_impls: BTreeSet<String>,
}

/// One relationship before crate-wide implementor filtering and graph analysis.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ErrorTypeSourceNode {
    type_path: String,
    #[getter(copy)]
    probe_id: InternalErrorTypeProbeId,
    source_target: Option<String>,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
}

impl ErrorTypeSourceNode {
    /// Start a builder for this value.
    #[tracing::instrument(level = "trace")]
    pub fn builder() -> ErrorTypeSourceNodeBuilder {
        ErrorTypeSourceNodeBuilder::default()
    }
}

impl SourceErrorTypes {
    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    pub(crate) fn native_file(
        file: &crate::loader::DatabaseSource<'_>,
        tree: &Path,
    ) -> CordialResult<ErrorTypesFile> {
        let (nodes, error_impls) = collect_native(file, tree)?;
        Ok(ErrorTypesFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            nodes,
            error_impls,
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<ErrorTypesFile>,
    ) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        if tree.is_dir() {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "rs")
                    && !path_has_fixtures(entry.path(), root)
                {
                    files.push(collect(entry.path(), &tree)?);
                }
            }
        }
        files.sort_by(|a, b| a.snapshot.path().cmp(b.snapshot.path()));
        Ok(Self {
            crate_root: root.to_path_buf(),
            files,
        })
    }

    /// Finalize relationships using only graph evidence, including implementors
    /// declared in other source files. No syntax or filesystem access occurs.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    pub fn report(
        &self,
        crate_name: &str,
        relative_paths: bool,
    ) -> CordialResult<InternalErrorTypeGraphReport> {
        let implementors: BTreeSet<_> = self
            .files
            .iter()
            .flat_map(|file| file.error_impls.iter().cloned())
            .collect();
        let raw = self
            .files
            .iter()
            .flat_map(|file| file.nodes.iter())
            .filter(|node| type_path_is_error_related(node.type_path(), &implementors))
            .cloned()
            .collect();
        let mut nodes = finalize_type_graph(raw, crate_name)?;
        if relative_paths {
            for node in &mut nodes {
                node.strip_file_prefix(&self.crate_root);
            }
        }
        Ok(InternalErrorTypeGraphReport::new(
            crate_name.to_string(),
            nodes,
        ))
    }
}

struct Fact {
    path: String,
    probe: InternalErrorTypeProbeId,
    target: Option<String>,
    node: SyntaxNode,
    snippet: String,
}

#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
#[hotpath::measure]
fn collect_native(
    file: &DatabaseSource<'_>,
    tree: &Path,
) -> CordialResult<(Vec<ErrorTypeSourceNode>, BTreeSet<String>)> {
    let prefix = module_path_from_src_file(tree, file.path());
    let mut nodes = Vec::new();
    let mut implementors = BTreeSet::new();
    let mut walk = file.tree().syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if excluded_module(&node) {
            walk.skip_subtree();
            continue;
        }
        let facts = if let Some(item) = ast::Struct::cast(node.clone()) {
            if declarations::derives_error(&item)
                && let Some(name) = item.name()
            {
                implementors.insert(name.text().to_string());
            }
            declarations::wrapper(&item, &prefix)
        } else if let Some(item) = ast::Enum::cast(node.clone()) {
            if declarations::derives_error(&item)
                && let Some(name) = item.name()
            {
                implementors.insert(name.text().to_string());
            }
            variants::collect(&item)
        } else if let Some(item) = ast::Impl::cast(node) {
            declarations::source_impl(&item, &mut implementors)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        for fact in facts {
            nodes.push(
                ErrorTypeSourceNode::builder()
                    .type_path(fact.path)
                    .probe_id(fact.probe)
                    .source_target(fact.target)
                    .file(file.path().to_path_buf())
                    .line(file.line(fact.node.text_range().start()))
                    .snippet(fact.snippet)
                    .build()?,
            );
        }
    }
    Ok((nodes, implementors))
}

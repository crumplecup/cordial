//! Snapshot-bound chain evidence issued by the shared source loader.

use crate::etiquettes::error_chain::{ErrorChainProbeId, ErrorChainRecord};
use crate::ir::error_sites_source::native::{context, excluded_module};
use crate::loader::module_path_from_src_file;
use crate::loader::{DatabaseSource, path_has_fixtures};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{AstNode, SyntaxNode, WalkEvent, ast};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod expressions;
pub(super) mod types;

/// Required crate-wide chain inventory on the shared graph root.
pub const ATTR_SOURCE_ERROR_CHAINS: &str = "source_error_chains";

/// Complete chain evidence, including source files with no records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceErrorChains {
    crate_root: PathBuf,
    files: Vec<ErrorChainsFile>,
}

/// Chain records bound to their original source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorChainsFile {
    snapshot: FileSnapshot,
    records: Vec<ErrorChainRecord>,
}

impl SourceErrorChains {
    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<ErrorChainsFile>,
    ) -> CordialResult<Self> {
        let tree = root.join("src");
        let mut files = Vec::new();
        if tree.exists() {
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

    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    pub(crate) fn native_file(
        file: &DatabaseSource<'_>,
        tree: &Path,
        root: &Path,
    ) -> CordialResult<ErrorChainsFile> {
        Ok(ErrorChainsFile {
            snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            records: collect_native(file, tree, root)?,
        })
    }

    /// Read deterministic rows without consulting syntax or the filesystem.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn records(&self) -> Vec<ErrorChainRecord> {
        let mut records: Vec<_> = self
            .files
            .iter()
            .flat_map(|file| file.records.clone())
            .collect();
        records.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
        });
        records
    }
}

struct Fact {
    rule: ErrorChainProbeId,
    node: SyntaxNode,
    snippet: String,
    foreign: Option<String>,
}

#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
#[hotpath::measure]
fn collect_native(
    file: &DatabaseSource<'_>,
    tree: &Path,
    root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    let prefix = module_path_from_src_file(tree, file.path());
    let mut records = Vec::new();
    let mut walk = file.tree().syntax().preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(node) = event else {
            continue;
        };
        if excluded_module(&node) {
            walk.skip_subtree();
            continue;
        }
        let facts = types::collect(&node)
            .into_iter()
            .chain(expressions::collect(&node));
        for fact in facts {
            // A From bridge is recorded before entering its own impl context.
            let owner = if ast::Impl::can_cast(fact.node.kind()) {
                fact.node.parent().unwrap_or_else(|| fact.node.clone())
            } else {
                fact.node.clone()
            };
            records.push(
                ErrorChainRecord::builder()
                    .rule_id(fact.rule)
                    .context(context(&owner, &prefix))
                    .file(
                        file.path()
                            .strip_prefix(root)
                            .unwrap_or(file.path())
                            .to_path_buf(),
                    )
                    .line(file.line(fact.node.text_range().start()))
                    .snippet(fact.snippet)
                    .foreign_error_type(fact.foreign)
                    .build()?,
            );
        }
    }
    Ok(records)
}

//! Panic policy over canonical source carriers. Parsing stays in the loader.
use super::{Context, PanicProofGraph, Scanner, SourcePanics, assertions, proof, syntax};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasName};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl SourcePanics {
    #[tracing::instrument(level = "debug", skip(files, verus), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourcePanics")]
    pub(crate) fn collect(
        root: &Path,
        files: &[(PathBuf, DatabaseSource<'_>)],
        #[cfg(feature = "verus_ir")] verus: &crate::VerusCrateIr,
    ) -> CordialResult<Self> {
        let proof_graph = PanicProofGraph::merge(
            files
                .iter()
                .map(|(_, file)| file_proofs(file))
                .collect::<Vec<_>>()
                .iter(),
        );
        let reachable = proof_graph.reachable();
        let mut records = Vec::new();
        let mut snapshots = Vec::new();
        for (tree, file) in files {
            let scanned = file_scan(root, tree, file, &reachable)?;
            snapshots.push(scanned.snapshot);
            records.extend(scanned.records);
        }
        #[cfg(feature = "verus_ir")]
        records.extend(crate::etiquettes::panics::verus_findings(verus, root)?);
        Ok(Self::from_parts(root, snapshots, proof_graph, records))
    }

    /// The inventory of a crate whose per-file parts are collected: the records
    /// in report order.
    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) fn from_parts(
        root: &Path,
        snapshots: Vec<FileSnapshot>,
        proof_graph: PanicProofGraph,
        mut records: Vec<crate::etiquettes::panics::PanicSiteRecord>,
    ) -> Self {
        records.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.context().cmp(b.context()))
                .then(a.snippet().cmp(b.snippet()))
        });
        Self::new(root.to_path_buf(), snapshots, proof_graph, records)
    }
}

/// The functions one file defines and the calls they make, for the proofs'
/// reachable set.
#[tracing::instrument(level = "debug", skip(file))]
pub(crate) fn file_proofs(file: &DatabaseSource<'_>) -> PanicProofGraph {
    proof::ProofCollector::of_file(file)
}

/// What scanning one file found, with the file's fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileScan {
    pub(crate) snapshot: FileSnapshot,
    pub(crate) records: Vec<crate::etiquettes::panics::PanicSiteRecord>,
}

/// One file's panic sites, given the functions the proofs reach: a function of
/// the file's syntax and that set.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn file_scan(
    root: &Path,
    tree: &Path,
    file: &DatabaseSource<'_>,
    reachable: &BTreeSet<String>,
) -> CordialResult<FileScan> {
    let context = Context {
        modules: module_path_from_src_file(tree, file.path()),
        ..Context::default()
    };
    let mut scan = Scanner::new(file, root, reachable, false);
    scan.walk(file.tree().syntax(), &context)?;
    Ok(FileScan {
        snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
        records: scan.records,
    })
}

impl Context {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn label(&self) -> String {
        let mut parts = self.modules.clone();
        parts.extend(self.implementation.clone());
        parts.extend(self.functions.clone());
        if parts.is_empty() {
            "<crate>".into()
        } else {
            parts.join("::")
        }
    }
}

impl<'a, 'db> Scanner<'a, 'db> {
    #[tracing::instrument(level = "debug", skip(source, reachable))]
    #[hotpath::measure]
    pub(super) fn new(
        source: &'a DatabaseSource<'db>,
        root: &'a Path,
        reachable: &'a BTreeSet<String>,
        embedded: bool,
    ) -> Self {
        Self {
            source,
            root,
            reachable,
            embedded,
            assertion_lines: BTreeSet::new(),
            records: Vec::new(),
        }
    }

    #[tracing::instrument(level = "debug", skip(self, node, inherited), err(level = "warn"))]
    #[hotpath::measure]
    pub(super) fn walk(&mut self, node: &SyntaxNode, inherited: &Context) -> CordialResult<()> {
        let context = if let Some(module) = ast::Module::cast(node.clone()) {
            let mut context = inherited.clone();
            if let Some(name) = module.name() {
                context.modules.push(name.text().to_string());
            }
            context.cfg_test |= syntax::cfg_flag(&module, "test");
            Some(context)
        } else if let Some(implementation) = ast::Impl::cast(node.clone()) {
            let mut context = inherited.clone();
            context.implementation = implementation.self_ty().as_ref().map(syntax::type_label);
            context.cfg_test |= syntax::cfg_flag(&implementation, "test");
            Some(context)
        } else if let Some(function) = ast::Fn::cast(node.clone())
            && syntax::tracked_function(&function)
        {
            let mut context = inherited.clone();
            if let Some(name) = function.name() {
                context.functions.push(name.text().to_string());
            }
            context.cfg_test |= syntax::cfg_flag(&function, "test");
            Some(context)
        } else if let Some(block) = ast::BlockExpr::cast(node.clone()) {
            self.assertion_lines
                .extend(assertions::error_assertion_lines(self.source, &block));
            let mut context = inherited.clone();
            if syntax::cfg_flag(&block, "not(kani)") {
                context.not_kani = true;
            } else if syntax::cfg_flag(&block, "kani") {
                context.not_kani = false;
            }
            Some(context)
        } else {
            None
        };
        let context = context.as_ref().unwrap_or(inherited);
        if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            self.method(&call, context)?;
        }
        if let Some(invocation) = ast::MacroCall::cast(node.clone()) {
            self.invocation(&invocation, context)?;
            return Ok(());
        }
        if let Some(literal) = ast::Literal::cast(node.clone()) {
            self.embedded_program(&literal, context)?;
        }
        for child in node.children() {
            self.walk(&child, context)?;
        }
        Ok(())
    }
}

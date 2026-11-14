//! Durable proof clauses selected from canonical Rust and memoized dialect islands.
use super::{
    ContractClause,
    index::{is_trivial, verifier_for_crate},
    make_finding, site_context,
    token_tree::{parse, render},
};
use crate::{
    CordialResult, FileSnapshot,
    loader::{DatabaseSource, module_path_from_src_file},
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasModuleItem},
};
use std::path::{Path, PathBuf};
mod kani;
mod walk;

#[derive(serde::Serialize, serde::Deserialize)]
/// One file's contract clauses, or `None` for a file this policy does not read:
/// outside `src`, or under a `gallery` directory. The snapshot is the file's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContractFile {
    snapshot: FileSnapshot,
    clauses: Vec<ContractClause>,
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn file_clauses(
    root: &Path,
    crate_name: &str,
    source: &DatabaseSource<'_>,
) -> CordialResult<Option<ContractFile>> {
    let Some(verifier) = verifier_for_crate(crate_name) else {
        return Ok(None);
    };
    let src = root.join("src");
    let Ok(relative) = source.path().strip_prefix(&src) else {
        return Ok(None);
    };
    if relative
        .components()
        .any(|part| part.as_os_str() == "gallery")
    {
        return Ok(None);
    }
    Ok(Some(ContractFile {
        snapshot: FileSnapshot::new(source.path().to_path_buf(), source.source_digest()),
        clauses: collect_file(source, &src, verifier)?,
    }))
}

/// The crate's clauses from each file's, in report order, with paths relative
/// to the crate root. Each file's snapshot is added to `snapshots`.
#[tracing::instrument(level = "debug", skip_all)]
#[hotpath::measure]
pub(crate) fn join_clauses<'a>(
    root: &Path,
    files: impl Iterator<Item = &'a ContractFile>,
    snapshots: &mut Vec<FileSnapshot>,
) -> Vec<ContractClause> {
    let mut clauses = Vec::new();
    for file in files {
        snapshots.push(file.snapshot.clone());
        clauses.extend(file.clauses.iter().cloned());
    }
    clauses.sort_by(|a, b| {
        a.site()
            .file()
            .cmp(b.site().file())
            .then(a.site().line().cmp(&b.site().line()))
            .then(a.site().snippet().cmp(b.site().snippet()))
    });
    for clause in &mut clauses {
        clause.strip_file_prefix(root);
    }
    clauses
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(in crate::etiquettes::antipatterns) fn collect(
    root: &Path,
    crate_name: &str,
    files: &[(PathBuf, &DatabaseSource<'_>)],
    snapshots: &mut Vec<FileSnapshot>,
) -> CordialResult<Vec<ContractClause>> {
    let mut found = Vec::new();
    for (_, source) in files {
        found.extend(file_clauses(root, crate_name, source)?);
    }
    Ok(join_clauses(root, found.iter(), snapshots))
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(super) fn collect_file(
    source: &DatabaseSource<'_>,
    src: &Path,
    verifier: &str,
) -> CordialResult<Vec<ContractClause>> {
    let mut clauses = Vec::new();
    let modules = module_path_from_src_file(src, source.path());
    if verifier == "verus" {
        for item in source.tree().items() {
            if let ast::Item::MacroCall(invocation) = item
                && name(invocation.path()).as_deref() == Some("verus")
                && let Some(tree) = invocation.token_tree()
            {
                clauses.extend(source.antipattern_contracts(&tree, &modules, "", "verus"));
            }
        }
    } else {
        walk::collect(
            source,
            source.tree().syntax(),
            &modules,
            verifier,
            &[],
            false,
            &mut clauses,
        )?;
    }
    Ok(clauses)
}

/// Called only by the loader's tracked opaque-dialect query.
#[tracing::instrument(level = "debug", skip_all)]
#[hotpath::measure]
pub(crate) fn collect_payload(
    text: &str,
    file: &Path,
    modules: &[String],
    context: &str,
    kind: &str,
    line: u32,
) -> Option<Vec<ContractClause>> {
    let tokens = parse(text)?;
    if kind == "verus" {
        return super::verus::collect_payload(&tokens, file, modules).ok();
    }
    let normalized = render(&tokens);
    if is_trivial(&normalized) {
        return Some(Vec::new());
    }
    let site = make_finding(site_context(modules, context), file, line, &normalized).ok()?;
    Some(vec![ContractClause::collect(
        site,
        "creusot",
        kind,
        tokens,
        Vec::new(),
    )])
}

#[tracing::instrument(level = "trace", skip(path))]
fn name(path: Option<ast::Path>) -> Option<String> {
    path?
        .segment()?
        .name_ref()
        .map(|name| name.text().to_string())
}

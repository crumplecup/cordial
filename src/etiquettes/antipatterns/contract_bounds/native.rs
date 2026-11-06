//! Durable proof clauses selected from canonical Rust and memoized dialect islands.
use super::{
    ContractClause,
    index::{is_trivial, normalize_tokens, verifier_for_crate},
    make_finding, site_context,
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

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
pub(in crate::etiquettes::antipatterns) fn collect_native(
    root: &Path,
    crate_name: &str,
    files: &[(PathBuf, &DatabaseSource<'_>)],
    snapshots: &mut Vec<FileSnapshot>,
) -> CordialResult<Vec<ContractClause>> {
    let Some(verifier) = verifier_for_crate(crate_name) else {
        return Ok(Vec::new());
    };
    let src = root.join("src");
    let mut clauses = Vec::new();
    for (_, source) in files {
        let Ok(relative) = source.path().strip_prefix(&src) else {
            continue;
        };
        if relative
            .components()
            .any(|part| part.as_os_str() == "gallery")
        {
            continue;
        }
        snapshots.push(FileSnapshot::new(
            source.path().to_path_buf(),
            source.source_digest(),
        ));
        clauses.extend(collect_file(source, &src, verifier)?);
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
    Ok(clauses)
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
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
pub(crate) fn collect_payload(
    tokens: proc_macro2::TokenStream,
    file: &Path,
    modules: &[String],
    context: &str,
    kind: &str,
    line: u32,
) -> Option<Vec<ContractClause>> {
    if kind == "verus" {
        return super::verus::collect_payload(tokens, file, modules).ok();
    }
    let normalized = normalize_tokens(tokens.clone());
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

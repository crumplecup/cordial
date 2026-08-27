//! CLI layout: clap types and dispatch live in the library; `main` is thin.
//!
//! Bin-only crates (no `lib.rs`) are out of scope. Library-only crates are too.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Item, ItemFn};

use crate::error::CordialResult;
use crate::loader::path_has_fixtures;

use super::tree::collect_mod_tree;
use super::types::{CliLayoutId, CliLayoutRecord};

pub(super) mod catalog;
mod idents;

use super::hunt::{finalize_acts, nested_clap_types};
use catalog::{LayoutCatalog, TypeRec, load_file};

use tracing::instrument;

/// Scan lib+bin crates for CLI-island / missing `act` / fat `main`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_cli_layout(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<CliLayoutRecord>> {
    let src_root = crate_root.join("src");
    let lib = src_root.join("lib.rs");
    if !lib.is_file() {
        return Ok(Vec::new());
    }
    let main = src_root.join("main.rs");
    let bin_dir = src_root.join("bin");
    if !main.is_file() && !bin_dir.is_dir() {
        return Ok(Vec::new());
    }

    let lib_files = collect_tree(crate_root, &lib)?;
    let mut bin_files = BTreeSet::new();
    let mut entry_files = Vec::new();
    if main.is_file() {
        bin_files.extend(collect_tree(crate_root, &main)?);
        entry_files.push(main.clone());
    }
    if bin_dir.is_dir() {
        for path in rust_files_in(&bin_dir, crate_root)? {
            bin_files.extend(collect_tree(crate_root, &path)?);
            if path.parent() == Some(bin_dir.as_path()) {
                entry_files.push(path);
            }
        }
    }
    let bin_only: BTreeSet<PathBuf> = bin_files.difference(&lib_files).cloned().collect();

    let mut catalog = LayoutCatalog {
        crate_name: crate_name.to_string(),
        types: BTreeMap::new(),
        acts: BTreeMap::new(),
        pending_acts: Vec::new(),
        free_fns: Vec::new(),
    };
    for path in lib_files.iter().chain(bin_only.iter()) {
        load_file(&mut catalog, path, lib_files.contains(path))?;
    }
    finalize_acts(&mut catalog);

    let mut findings = Vec::new();
    let clap_idents: BTreeSet<String> = catalog
        .types
        .values()
        .filter(|item| item.parser || item.subcommand)
        .map(|item| item.ident.clone())
        .collect();
    let has_clap = !clap_idents.is_empty();

    for item in catalog.types.values() {
        if item.in_library {
            if item.parser || item.subcommand {
                lint_act(crate_name, item, &catalog, &clap_idents, &mut findings);
            }
            continue;
        }
        if item.parser || item.subcommand || item.error {
            findings.push(finding(
                crate_name,
                CliLayoutId::Island001,
                item.type_path.clone(),
                item.file.clone(),
                item.line,
                format!(
                    "{} — CLI and error types belong in the library, not a binary island",
                    item.snippet
                ),
            ));
        }
    }

    if has_clap {
        for func in &catalog.free_fns {
            if !func.in_library {
                continue;
            }
            for ident in &func.input_idents {
                if clap_idents.contains(ident) {
                    findings.push(finding(
                        crate_name,
                        CliLayoutId::Act001,
                        format!("{}::{ident}", catalog.crate_name),
                        func.file.clone(),
                        func.line,
                        format!(
                            "fn {} — dispatch `{ident}` with `{ident}::act`, not a free function",
                            func.name
                        ),
                    ));
                    break;
                }
            }
        }
        for path in &entry_files {
            lint_thin_main(crate_name, path, &mut findings)?;
        }
    }

    Ok(findings)
}

#[instrument(level = "debug", skip(item, catalog, clap_idents, findings))]
fn lint_act(
    crate_name: &str,
    item: &TypeRec,
    catalog: &LayoutCatalog,
    clap_idents: &BTreeSet<String>,
    findings: &mut Vec<CliLayoutRecord>,
) {
    let Some(act) = catalog.acts.get(&item.ident) else {
        findings.push(finding(
            crate_name,
            CliLayoutId::Act001,
            item.type_path.clone(),
            item.file.clone(),
            item.line,
            format!(
                "{} — write `fn act(self, …) -> Result<_, _>` on this clap type",
                item.snippet
            ),
        ));
        return;
    };
    let nested = nested_clap_types(item, clap_idents);
    let missing: Vec<String> = nested
        .into_iter()
        .filter(|name| !act.called_on.contains(name))
        .collect();
    if missing.is_empty() {
        return;
    }
    let names = missing.join(", ");
    findings.push(finding(
        crate_name,
        CliLayoutId::Act001,
        item.type_path.clone(),
        act.file.clone(),
        act.line,
        format!(
            "{}::act must call `act` on nested clap type(s) `{names}`",
            item.ident
        ),
    ));
}

#[instrument(level = "debug", err(level = "warn"))]
fn collect_tree(crate_root: &Path, root: &Path) -> CordialResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    collect_mod_tree(root, crate_root, &mut files)?;
    Ok(files)
}

#[instrument(level = "debug", err(level = "warn"))]
fn rust_files_in(dir: &Path, crate_root: &Path) -> CordialResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path_has_fixtures(path, crate_root) {
            continue;
        }
        files.push(path.to_path_buf());
    }
    Ok(files)
}
#[instrument(level = "debug", skip(file, findings), err(level = "warn"))]
fn lint_thin_main(
    crate_name: &str,
    file: &Path,
    findings: &mut Vec<CliLayoutRecord>,
) -> CordialResult<()> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    for item in &syntax.items {
        match item {
            Item::Use(_) | Item::ExternCrate(_) => {}
            Item::Fn(func) if func.sig.ident == "main" => {
                lint_main_fn(crate_name, file, func, findings);
            }
            Item::Fn(func) => findings.push(finding(
                crate_name,
                CliLayoutId::Main001,
                format!("{}::{}", crate_name, func.sig.ident),
                file.to_path_buf(),
                func.span().start().line as u32,
                format!(
                    "fn {} — `main` should parse, call `act`, and convert to miette",
                    func.sig.ident
                ),
            )),
            Item::Mod(module) => findings.push(finding(
                crate_name,
                CliLayoutId::Main001,
                format!("{}::{}", crate_name, module.ident),
                file.to_path_buf(),
                module.span().start().line as u32,
                format!(
                    "mod {} — dispatch and CLI types belong in the library",
                    module.ident
                ),
            )),
            Item::Struct(item) => findings.push(finding(
                crate_name,
                CliLayoutId::Main001,
                format!("{}::{}", crate_name, item.ident),
                file.to_path_buf(),
                item.span().start().line as u32,
                format!("struct {} — keep `main` thin", item.ident),
            )),
            Item::Enum(item) => findings.push(finding(
                crate_name,
                CliLayoutId::Main001,
                format!("{}::{}", crate_name, item.ident),
                file.to_path_buf(),
                item.span().start().line as u32,
                format!("enum {} — keep `main` thin", item.ident),
            )),
            Item::Impl(item) => findings.push(finding(
                crate_name,
                CliLayoutId::Main001,
                crate_name.to_string(),
                file.to_path_buf(),
                item.span().start().line as u32,
                "impl in `main` — dispatch belongs on library types".to_string(),
            )),
            _ => {}
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip(file, func, findings))]
fn lint_main_fn(crate_name: &str, file: &Path, func: &ItemFn, findings: &mut Vec<CliLayoutRecord>) {
    let mut hunt = MainHunt {
        has_match: false,
        has_parse: false,
        has_act: false,
    };
    hunt.visit_block(&func.block);
    // Extra calls are allowed so `main` can invoke the library tracing-subscriber
    // helper once before `parse` / `act`. Extra *items* in this file still fail.
    if hunt.has_match {
        findings.push(finding(
            crate_name,
            CliLayoutId::Main001,
            format!("{crate_name}::main"),
            file.to_path_buf(),
            func.span().start().line as u32,
            "match in `main` — dispatch with `Cli::act`, not in `main`".to_string(),
        ));
    }
    if !hunt.has_parse || !hunt.has_act {
        findings.push(finding(
            crate_name,
            CliLayoutId::Main001,
            format!("{crate_name}::main"),
            file.to_path_buf(),
            func.span().start().line as u32,
            "`main` must call `parse` and `act` (then miette)".to_string(),
        ));
    }
}

struct MainHunt {
    has_match: bool,
    has_parse: bool,
    has_act: bool,
}

impl<'ast> Visit<'ast> for MainHunt {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.has_match = true;
        syn::visit::visit_expr_match(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "parse" {
            self.has_parse = true;
        }
        if node.method == "act" {
            self.has_act = true;
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref()
            && let Some(last) = path.path.segments.last()
        {
            if last.ident == "parse" {
                self.has_parse = true;
            }
            if last.ident == "act" {
                self.has_act = true;
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}
#[instrument(level = "debug", skip(rule_id, file))]
fn finding(
    crate_name: &str,
    rule_id: CliLayoutId,
    context: String,
    file: PathBuf,
    line: u32,
    snippet: String,
) -> CliLayoutRecord {
    CliLayoutRecord {
        crate_name: crate_name.to_string(),
        rule_id,
        context,
        file,
        line,
        snippet,
    }
}

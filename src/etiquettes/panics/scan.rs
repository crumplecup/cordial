//! syn-based scan for panic, unreachable, expect, unwrap, and compile_error sites.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Expr, ExprLit, ExprMacro, ExprMethodCall, Item, ItemFn, ItemImpl, ItemMod, Lit, Macro, Type,
};

use super::error_assertion;
use super::kani_reach::{KaniReachability, build_kani_reachability};
use super::types::{PanicKind, PanicSiteRecord};
#[cfg(not(feature = "verus_ir"))]
use super::verus_recover::{VerusFunctionChunk, collect_verus_functions};
#[cfg(feature = "verus_ir")]
use super::verus_reach;
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use tracing::instrument;
/// Scan `src/` and `tests/` under `crate_root`, excluding `fixtures/` paths.
///
/// Parses every file in the crate once up front to build a
/// `#[kani::proof]` reachability closure (see `kani_reach`'s own doc
/// comment) before scanning any of them for panic sites, since a site
/// that's part of a proof's own failure mechanism must never be flagged
/// regardless of which file it's found in relative to which file
/// declares the harness that reaches it.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_panics(crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
    let mut parsed = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        parsed.extend(parse_source_tree(&tree_root, crate_root)?);
    }
    let reachability = build_kani_reachability(parsed.iter().map(|file| &file.syntax));

    let mut findings = Vec::new();
    for file in &parsed {
        findings.extend(scan_parsed_file(file, crate_root, &reachability));
    }
    #[cfg(feature = "verus_ir")]
    {
        let verus_ir = crate::verus_ir::scan_crate_verus_ir(crate_root)?;
        findings.extend(verus_ir_findings(&verus_ir, crate_root));
    }
    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.context.cmp(&b.context))
            .then(a.snippet.cmp(&b.snippet))
    });
    Ok(findings)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(src_root: &Path, crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
    let parsed = parse_source_tree(src_root, crate_root)?;
    let reachability = build_kani_reachability(parsed.iter().map(|file| &file.syntax));
    let mut findings = Vec::new();
    for file in &parsed {
        findings.extend(scan_parsed_file(file, crate_root, &reachability));
    }
    Ok(findings)
}

/// One parsed source file plus the tree root it was discovered under
/// (needed to derive its crate-relative module path).
struct ParsedFile {
    path: PathBuf,
    src_root: PathBuf,
    syntax: syn::File,
}

#[instrument(level = "debug", err(level = "warn"))]
fn parse_source_tree(src_root: &Path, crate_root: &Path) -> CordialResult<Vec<ParsedFile>> {
    let mut parsed = Vec::new();
    if !src_root.is_dir() {
        return Ok(parsed);
    }

    for entry in walkdir::WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        let syntax = syn::parse_file(&source).map_err(|err| {
            crate::error::CordialError::syn_parse(path.display().to_string(), err)
        })?;
        parsed.push(ParsedFile {
            path: path.to_path_buf(),
            src_root: src_root.to_path_buf(),
            syntax,
        });
    }

    Ok(parsed)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<PanicSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let reachability = build_kani_reachability(std::iter::once(&syntax));
    let parsed = ParsedFile {
        path: file.to_path_buf(),
        src_root: src_root.to_path_buf(),
        syntax,
    };
    let findings = scan_parsed_file(&parsed, crate_root, &reachability);
    #[cfg(feature = "verus_ir")]
    let findings = {
        let mut findings = findings;
        let module_path = module_path_from_src_file(src_root, file).join("::");
        let verus_ir = crate::verus_ir::scan_verus_rust_source(source, file, &module_path);
        findings.extend(verus_ir_findings(&verus_ir, crate_root));
        findings
    };
    Ok(findings)
}

/// Convert every panic site [`crate::verus_ir::scan_crate_verus_ir`]/
/// [`crate::verus_ir::scan_verus_rust_source`] found -- a real, complete
/// parse -- into this etiquette's own [`PanicSiteRecord`] shape, so a
/// real consumer can't tell whether a given finding came from the
/// ordinary `syn`-based walk or from `verus_syn`. Kani-reachability
/// exemption doesn't apply here: `amenable_verus`-shaped crates never
/// mix Kani proof harnesses into the same crate as `verus!` content
/// (verifier backends never depend on each other), so there is nothing
/// for a `verus!` site to be reachable *from* in the sense `kani_reach`
/// checks. Two Verus-side analogs do apply, though: a site `verus_ir`
/// marked [`proven_unreachable_by_ghost_sibling`](
/// crate::verus_ir::VerusPanicSite::proven_unreachable_by_ghost_sibling)
/// is that branch's own real, SMT-checked failure mechanism seen only
/// by the ordinary-rustc fallback arm; and a whole function
/// [`verus_reach`] determines is a verification leaf (a real `ensures`
/// clause, called by nothing else locally) is itself the checked claim,
/// not library API surface -- the exact same "must never be flagged"
/// reasoning `kani_reach` applies to a Kani harness's own panic, just
/// keyed on structural facts Verus's own grammar makes visible instead
/// of call-graph reachability from a `#[kani::proof]` root.
#[cfg(feature = "verus_ir")]
#[instrument(level = "debug", skip(ir))]
fn verus_ir_findings(ir: &crate::verus_ir::VerusCrateIr, crate_root: &Path) -> Vec<PanicSiteRecord> {
    let reachability = verus_reach::build_verus_reachability(ir);
    ir.functions
        .iter()
        .filter(|function| !reachability.is_verification_leaf(&function.name))
        .flat_map(|function| {
            let context = format!("{}::{}", function.module_path, function.name);
            let file = function
                .span
                .file
                .strip_prefix(crate_root)
                .unwrap_or(&function.span.file)
                .to_path_buf();
            let cfg_test = function.cfg_test;
            function
                .panic_sites
                .iter()
                .filter(|site| !site.proven_unreachable_by_ghost_sibling)
                .map(move |site| PanicSiteRecord {
                    kind: verus_panic_kind(site.kind),
                    context: context.clone(),
                    file: file.clone(),
                    line: site.line,
                    snippet: site.snippet.clone(),
                    cfg_test,
                })
        })
        .collect()
}

#[cfg(feature = "verus_ir")]
#[instrument(level = "trace", ret)]
fn verus_panic_kind(kind: crate::verus_ir::VerusPanicKind) -> PanicKind {
    match kind {
        crate::verus_ir::VerusPanicKind::Panic => PanicKind::Panic,
        crate::verus_ir::VerusPanicKind::Unreachable => PanicKind::Unreachable,
        crate::verus_ir::VerusPanicKind::Expect => PanicKind::Expect,
        crate::verus_ir::VerusPanicKind::Unwrap => PanicKind::Unwrap,
        crate::verus_ir::VerusPanicKind::CompileError => PanicKind::CompileError,
    }
}

#[instrument(level = "debug", skip(file, reachability))]
fn scan_parsed_file(
    file: &ParsedFile,
    crate_root: &Path,
    reachability: &KaniReachability,
) -> Vec<PanicSiteRecord> {
    let module_prefix = module_path_from_src_file(&file.src_root, &file.path);
    let mut visitor = PanicScanVisitor {
        file: file.path.clone(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_cfg_test: false,
        in_cfg_not_kani: false,
        reachability,
        findings: Vec::new(),
        exempt_error_assertion_lines: std::collections::HashSet::new(),
    };
    visitor.visit_file(&file.syntax);
    visitor.findings
}

struct PanicScanVisitor<'a> {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    in_cfg_test: bool,
    /// True inside a `#[cfg(not(kani))]` block -- overrides
    /// `reachability`, since that code never runs during Kani
    /// verification even when its enclosing function is otherwise
    /// reachable from a `#[kani::proof]` harness (see e.g.
    /// `symbolic_any`'s two-branch shape in amenable_kani::compose).
    in_cfg_not_kani: bool,
    reachability: &'a KaniReachability,
    findings: Vec<PanicSiteRecord>,
    /// Line numbers of `.expect_err(..)`/`.unwrap_err()` calls that
    /// assert a fact against the extracted error value rather than
    /// discarding/propagating it -- see `error_assertion`'s own doc
    /// comment. Accumulates across every `Block` visited in the file
    /// (line numbers are unique within one file), populated by
    /// `visit_block` before it descends into that block's own
    /// statements.
    exempt_error_assertion_lines: std::collections::HashSet<u32>,
}

impl PanicScanVisitor<'_> {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        let mut parts = self.module_prefix.clone();
        if let Some(ty) = &self.impl_type {
            parts.push(ty.clone());
        }
        parts.extend(self.fn_stack.iter().cloned());
        if parts.is_empty() {
            "<crate>".to_string()
        } else {
            parts.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, kind))]
    fn push_finding(&mut self, kind: PanicKind, line: u32, snippet: String) {
        // A site only reachable from a #[kani::proof] harness (or nested
        // inside #[cfg(kani)]) is that harness's own failure mechanism --
        // Kani checks reachable panics/asserts, not a harness's return
        // value, so routing this through Result would silently disable
        // the check. See kani_reach's own doc comment.
        if !self.in_cfg_not_kani
            && self
                .fn_stack
                .last()
                .is_some_and(|key| self.reachability.contains(key))
        {
            return;
        }
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        if kind == PanicKind::Unwrap
            && self.findings.last().is_some_and(|last| {
                last.kind == PanicKind::Unwrap && last.line == line && last.file == file
            })
        {
            return;
        }
        self.findings.push(PanicSiteRecord {
            kind,
            context: self.site_context(),
            file,
            line,
            snippet,
            cfg_test: self.in_cfg_test,
        });
    }

    #[instrument(level = "debug", skip(self, mac))]
    fn check_macro(&mut self, mac: &Macro) {
        if mac.path.is_ident("verus") {
            // When the `verus_ir` feature is available, `scan_crate_panics`/
            // `scan_rust_source` already merge in real, complete findings
            // from a genuine `verus_syn` parse (see this module's own
            // `verus_ir_findings`) -- skip the best-effort recovery here
            // entirely rather than double-count.
            #[cfg(not(feature = "verus_ir"))]
            self.scan_verus_chunks(collect_verus_functions(mac.tokens.clone()));
            return;
        }
        let Some(kind) = macro_panic_kind(&mac.path) else {
            return;
        };
        self.push_finding(kind, mac.span().start().line as u32, macro_snippet(mac));
    }

    /// Parse each recovered `verus! { .. }` function chunk as a real
    /// `syn::Block` and visit it with this same visitor -- `fn_stack`
    /// gets the chunk's real name pushed first, so findings inside get
    /// the same accurate context (and the same Kani-reachability/
    /// cfg(test) handling) as an ordinary function would. A chunk that
    /// fails to parse (genuine Verus-only expression syntax -- see
    /// `verus_recover`'s own doc comment) is retried one level deeper
    /// via `collect_verus_functions` on its own body tokens, to still
    /// opportunistically find a nested `fn` even inside an outer shell
    /// this scanner can't fully make sense of. Only compiled in when
    /// `verus_ir` (a genuinely complete parse) isn't available.
    #[cfg(not(feature = "verus_ir"))]
    #[instrument(level = "debug", skip(self, chunks))]
    fn scan_verus_chunks(&mut self, chunks: Vec<VerusFunctionChunk>) {
        for chunk in chunks {
            let braced = proc_macro2::TokenStream::from(proc_macro2::TokenTree::Group(
                proc_macro2::Group::new(proc_macro2::Delimiter::Brace, chunk.body.clone()),
            ));
            match syn::parse2::<syn::Block>(braced) {
                Ok(block) => {
                    self.fn_stack.push(chunk.name);
                    syn::visit::visit_block(self, &block);
                    self.fn_stack.pop();
                }
                Err(_) => {
                    self.scan_verus_chunks(collect_verus_functions(chunk.body));
                }
            }
        }
    }

    #[instrument(level = "debug", skip(self, call))]
    fn check_method_call(&mut self, call: &ExprMethodCall) {
        let method = call.method.to_string();
        let line = call.span().start().line as u32;
        // Asserting a fact against the extracted error value (`let error
        // = ...expect_err(..); assert_eq!(error, ..)`) is that
        // assertion's own mechanism, not a discarded/propagated setup
        // failure -- see error_assertion's own doc comment. Only applies
        // to the `_err` methods: `.expect(..)`/`.unwrap()` assert the Ok
        // case succeeded, an entirely different situation this pattern
        // doesn't cover.
        let is_error_assertion = matches!(method.as_str(), "expect_err" | "unwrap_err")
            && self.exempt_error_assertion_lines.contains(&line);
        match method.as_str() {
            "expect" | "expect_err" if !is_error_assertion => {
                self.push_finding(PanicKind::Expect, line, expect_snippet(call))
            }
            "unwrap" | "unwrap_err" if !is_unwrap_variant(call) && !is_error_assertion => {
                self.push_finding(PanicKind::Unwrap, line, format!(".{}()", call.method))
            }
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&item_mod.attrs) {
            self.in_cfg_test = true;
        }
        let Some((_, items)) = &item_mod.content else {
            self.in_cfg_test = prev;
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
        self.in_cfg_test = prev;
    }
}

impl<'ast> Visit<'ast> for PanicScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev_cfg = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
        self.in_cfg_test = prev_cfg;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        self.check_macro(&node.mac);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.check_method_call(node);
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_block(&mut self, node: &'ast syn::ExprBlock) {
        let prev = self.in_cfg_not_kani;
        if has_cfg_not_flag(&node.attrs, "kani") {
            self.in_cfg_not_kani = true;
        } else if has_cfg_flag(&node.attrs, "kani") {
            self.in_cfg_not_kani = false;
        }
        syn::visit::visit_expr_block(self, node);
        self.in_cfg_not_kani = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.exempt_error_assertion_lines
            .extend(error_assertion::error_assertion_lines(node));
        syn::visit::visit_block(self, node);
    }
}

#[instrument(level = "trace", skip(call), ret)]
fn is_unwrap_variant(call: &ExprMethodCall) -> bool {
    matches!(
        call.method.to_string().as_str(),
        "unwrap_or" | "unwrap_or_else" | "unwrap_or_default"
    )
}

#[instrument(level = "trace", skip(attrs))]
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    has_cfg_flag(attrs, "test")
}

/// Whether `attrs` carries a bare `#[cfg(flag)]` (no `not(..)`, no
/// `all(..)`/`any(..)` combinators -- just the single flag name).
#[instrument(level = "trace", skip(attrs, flag))]
pub(super) fn has_cfg_flag(attrs: &[syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == flag
    })
}

/// Whether `attrs` carries a bare `#[cfg(not(flag))]`.
#[instrument(level = "trace", skip(attrs, flag))]
pub(super) fn has_cfg_not_flag(attrs: &[syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == format!("not({flag})")
    })
}

#[instrument(level = "debug", skip(path))]
fn macro_panic_kind(path: &syn::Path) -> Option<PanicKind> {
    let ident = path.segments.last()?.ident.to_string();
    match ident.as_str() {
        "panic" => Some(PanicKind::Panic),
        "unreachable" => Some(PanicKind::Unreachable),
        "compile_error" => Some(PanicKind::CompileError),
        _ => None,
    }
}

#[instrument(level = "debug", skip(mac))]
fn macro_snippet(mac: &Macro) -> String {
    let name = path_label(&mac.path);
    let args = mac.tokens.to_string();
    let trimmed = truncate_snippet(&args, 72);
    format!("{name}!({trimmed})")
}

#[instrument(level = "debug", skip(call))]
fn expect_snippet(call: &ExprMethodCall) -> String {
    if let Some(Expr::Lit(ExprLit {
        lit: Lit::Str(lit), ..
    })) = call.args.first()
    {
        return format!(".{}(\"{}\")", call.method, lit.value());
    }
    format!(".{}(…)", call.method)
}

#[instrument(level = "debug")]
fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

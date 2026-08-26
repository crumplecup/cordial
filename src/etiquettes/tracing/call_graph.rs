//! Workspace-wide call-graph facts: which functions are reachable
//! *only* from proof-only entry points -- functions nested inside an
//! ancestor `#[cfg(<verifier-cfg>)]`, the crate's own gate cfg name(s)
//! (see [`crate_gate_cfgs`](super::apply::crate_gate_cfgs)) -- so
//! instrumenting them can never produce observable output in any real
//! build, exactly like the entry points themselves.
//!
//! **Why this has to be call-graph-based, not trait-name-based.** A
//! first version of this recognized `amenable_core::Ensures`/`Requires`
//! impls by name specifically -- real, but a special case tied to one
//! workspace's own trait names, not a mechanism any other cordial user
//! gets for free. The actual, reusable invariant is call-graph
//! reachability: a function whose *every* real caller, transitively,
//! bottoms out in a proof-only entry point is exactly as dead-to-
//! tracing as the entry point itself, whatever trait (if any) it
//! happens to implement.
//!
//! **How.** No rustc integration, so no real type inference -- call
//! resolution is name-based: `Type::method(..)`/`bare_fn(..)` (explicit
//! path syntax) is unambiguous enough to match against a workspace-wide
//! registry of known function definitions by their own last one or two
//! path segments; `receiver.method(..)` calls, whose receiver's type
//! isn't known without real inference, are not resolved at all and so
//! never produce a graph edge -- a missed edge only risks under-
//! excluding (a function stays `Gated` that could have been `Skip`),
//! never the reverse. Fixed point: seed `excluded` with every function
//! nested in an ancestor gate cfg, then repeatedly add any function
//! whose in-workspace callers are **all** already `excluded` (and it
//! has at least one) until nothing changes. A function with zero known
//! in-workspace callers -- `pub` API an external crate might call, or
//! genuinely dead code -- is never added: no positive evidence it's
//! proof-only, and gating dead code costs nothing either way.
//!
//! Scoped to same-workspace calls only (no attempt to resolve a call
//! into an external, non-workspace dependency) -- every real case this
//! was built against (`amenable_kani`'s own `Ensures`/`Requires`
//! impls, called from its own proof harnesses) is intra-workspace.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use syn::visit::Visit;
use syn::{Expr, ImplItem, Item, ItemFn, ItemImpl, ItemMod};

use crate::config::TracingThresholds;
use crate::{PathInclusionFacts, workspace_path_inclusions};

use super::apply::crate_gate_cfgs;
use super::scan::{impl_method_local_name, syn_path_label, type_label};

use tracing::instrument;

/// One function's identity: which crate it's defined in, and its
/// qualified name the same way [`super::scan::scan_rust_source`]
/// records it (module-prefixed, trait-qualified for a trait impl
/// method).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FunctionId {
    crate_name: String,
    qualified_name: String,
}

/// Workspace-wide call-graph facts, computed once per session (cached
/// like [`PathInclusionFacts`]).
#[derive(Debug, Default, Clone)]
pub struct CallGraphFacts {
    /// Crate name -> qualified names never worth recording as needing
    /// `#[instrument]`, regardless of role/recipe.
    never_instrument: HashMap<String, HashSet<String>>,
}

static EMPTY: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();

impl CallGraphFacts {
    /// Qualified names in `crate_name` that are reachable only from
    /// proof-only entry points -- never worth recording as needing
    /// `#[instrument]`.
    #[instrument(level = "trace", skip(self))]
    pub fn never_instrument(&self, crate_name: &str) -> &HashSet<String> {
        self.never_instrument
            .get(crate_name)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }
}

type FactsCache = Mutex<Option<(PathBuf, CallGraphFacts)>>;
static FACTS_CACHE: FactsCache = Mutex::new(None);

/// Cached, workspace-wide call-graph facts -- computed once per
/// `workspace_root` per session, matching
/// [`crate::workspace_path_inclusions`]'s own cache shape.
#[instrument(level = "debug", skip(config))]
pub fn workspace_call_graph(workspace_root: &Path, config: &TracingThresholds) -> CallGraphFacts {
    let cache_key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    if let Ok(cache) = FACTS_CACHE.lock()
        && let Some((key, facts)) = cache.as_ref()
        && *key == cache_key
    {
        return facts.clone();
    }

    let path_facts = workspace_path_inclusions(workspace_root);
    let facts = compute_call_graph(config, &path_facts);
    if let Ok(mut cache) = FACTS_CACHE.lock() {
        *cache = Some((cache_key, facts.clone()));
    }
    facts
}

/// One discovered function definition, kept across both passes: first
/// to build the workspace-wide registry, then (once the registry is
/// complete) to resolve its own body's call sites against it.
struct CollectedFn {
    id: FunctionId,
    /// Registry key(s) a call site could use to reach this function:
    /// `Type::method`/`Trait::method` for impl methods (both, so a
    /// call written through either the type or an in-scope trait
    /// resolves), or the bare name for a free function.
    call_keys: Vec<String>,
    ancestor_seed: bool,
    body: Option<syn::Block>,
}

#[instrument(level = "debug", skip(config, path_facts))]
fn compute_call_graph(
    config: &TracingThresholds,
    path_facts: &PathInclusionFacts,
) -> CallGraphFacts {
    let mut collected: Vec<CollectedFn> = Vec::new();

    for (crate_name, crate_root) in path_facts.crate_roots() {
        let src_root = crate_root.join("src");
        if !src_root.is_dir() {
            continue;
        }
        let gate_cfgs: HashSet<String> = crate_gate_cfgs(crate_name, config, path_facts)
            .into_iter()
            .collect();
        for entry in walkdir::WalkDir::new(&src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(syntax) = syn::parse_file(&source) else {
                continue;
            };
            let module_prefix = crate::loader::module_path_from_src_file(&src_root, path);
            let mut visitor = CollectVisitor {
                crate_name: crate_name.to_string(),
                module_prefix,
                gate_cfgs: &gate_cfgs,
                collected: Vec::new(),
            };
            visitor.visit_module_items(&syntax.items);
            collected.extend(visitor.collected);
        }
    }

    // Registry: call key -> every collected function whose own
    // call_keys include it (workspace-wide, deliberately not crate-
    // scoped -- a call site's own text doesn't name which crate its
    // callee lives in).
    let mut registry: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, function) in collected.iter().enumerate() {
        for key in &function.call_keys {
            registry.entry(key.as_str()).or_default().push(index);
        }
    }

    let mut callers_of: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (caller_index, function) in collected.iter().enumerate() {
        let Some(body) = &function.body else {
            continue;
        };
        let mut call_visitor = CallSiteVisitor { calls: Vec::new() };
        call_visitor.visit_block(body);
        for key in call_visitor.calls {
            let Some(candidates) = registry.get(key.as_str()) else {
                continue;
            };
            // Ambiguous (>1 workspace-wide definition sharing this
            // exact key) -- don't guess which one a bare/short name
            // meant; an unresolved call never produces a false
            // exclusion, only a missed one.
            let [callee_index] = candidates[..] else {
                continue;
            };
            if callee_index != caller_index {
                callers_of
                    .entry(callee_index)
                    .or_default()
                    .insert(caller_index);
            }
        }
    }

    let mut excluded: HashSet<usize> = collected
        .iter()
        .enumerate()
        .filter(|(_, function)| function.ancestor_seed)
        .map(|(index, _)| index)
        .collect();
    loop {
        let mut added_any = false;
        for (index, callers) in &callers_of {
            if excluded.contains(index) {
                continue;
            }
            if !callers.is_empty() && callers.iter().all(|caller| excluded.contains(caller)) {
                excluded.insert(*index);
                added_any = true;
            }
        }
        if !added_any {
            break;
        }
    }

    let mut never_instrument: HashMap<String, HashSet<String>> = HashMap::new();
    for index in excluded {
        let id = &collected[index].id;
        never_instrument
            .entry(id.crate_name.clone())
            .or_default()
            .insert(id.qualified_name.clone());
    }
    CallGraphFacts { never_instrument }
}

struct CollectVisitor<'a> {
    crate_name: String,
    module_prefix: Vec<String>,
    gate_cfgs: &'a HashSet<String>,
    collected: Vec<CollectedFn>,
}

impl CollectVisitor<'_> {
    #[instrument(level = "trace", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Fn(item_fn) => self.record_free_fn(item_fn, false),
                Item::Mod(item_mod) => self.visit_mod(item_mod),
                Item::Impl(item_impl) => self.visit_impl(item_impl),
                Item::Macro(item_macro) => {
                    if let Some(nested) = trailing_item_block(&item_macro.mac) {
                        self.visit_module_items(&nested);
                    }
                }
                _ => {}
            }
        }
    }

    #[instrument(level = "debug", skip(self, item_fn))]
    fn record_free_fn(&mut self, item_fn: &ItemFn, ancestor_seed: bool) {
        let seed = ancestor_seed
            || has_cfg(&item_fn.attrs, self.gate_cfgs)
            || has_verifier_attr(&item_fn.attrs, self.gate_cfgs);
        let name = item_fn.sig.ident.to_string();
        self.collected.push(CollectedFn {
            id: FunctionId {
                crate_name: self.crate_name.clone(),
                qualified_name: self.qualify(&name),
            },
            call_keys: vec![name],
            ancestor_seed: seed,
            body: Some(item_fn.block.as_ref().clone()),
        });
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let seed = has_cfg(&item_mod.attrs, self.gate_cfgs);
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix.push(item_mod.ident.to_string());
        if seed {
            self.visit_module_items_seeded(items);
        } else {
            self.visit_module_items(items);
        }
        self.module_prefix = prev_prefix;
    }

    /// Same as [`Self::visit_module_items`], but every function found
    /// (including nested further) is unconditionally ancestor-seeded --
    /// used once a `#[cfg(<gate>)]`-nested module has already been
    /// entered.
    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items_seeded(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Fn(item_fn) => self.record_free_fn(item_fn, true),
                Item::Mod(item_mod) => {
                    let Some((_, nested)) = &item_mod.content else {
                        continue;
                    };
                    let prev_prefix = self.module_prefix.clone();
                    self.module_prefix.push(item_mod.ident.to_string());
                    self.visit_module_items_seeded(nested);
                    self.module_prefix = prev_prefix;
                }
                Item::Impl(item_impl) => self.visit_impl_seeded(item_impl, true),
                Item::Macro(item_macro) => {
                    if let Some(nested) = trailing_item_block(&item_macro.mac) {
                        self.visit_module_items_seeded(&nested);
                    }
                }
                _ => {}
            }
        }
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &ItemImpl) {
        let seed = has_cfg(&item_impl.attrs, self.gate_cfgs)
            || has_verifier_attr(&item_impl.attrs, self.gate_cfgs);
        self.visit_impl_seeded(item_impl, seed);
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl_seeded(&mut self, item_impl: &ItemImpl, ancestor_seed: bool) {
        let self_ty = type_label(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(_, path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local = impl_method_local_name(&self_ty, trait_name.as_deref(), &method.sig.ident);
            let seed = ancestor_seed
                || has_cfg(&method.attrs, self.gate_cfgs)
                || has_verifier_attr(&method.attrs, self.gate_cfgs);
            let mut call_keys = vec![format!("{self_ty}::{}", method.sig.ident)];
            if let Some(trait_name) = &trait_name {
                call_keys.push(format!("{trait_name}::{}", method.sig.ident));
            }
            self.collected.push(CollectedFn {
                id: FunctionId {
                    crate_name: self.crate_name.clone(),
                    qualified_name: self.qualify(&local),
                },
                call_keys,
                ancestor_seed: seed,
                body: Some(method.block.clone()),
            });
        }
    }
}

/// `true` when `attrs` includes a bare `#[cfg(name)]` where `name` is
/// in `gate_cfgs`. Only the bare form is recognized, not
/// `any()`/`not()`/`all()` combinators -- every real site found in
/// this workspace uses it.
#[instrument(level = "trace", skip(attrs, gate_cfgs), ret)]
fn has_cfg(attrs: &[syn::Attribute], gate_cfgs: &HashSet<String>) -> bool {
    if gate_cfgs.is_empty() {
        return false;
    }
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if let Some(ident) = meta.path.get_ident()
                && gate_cfgs.contains(&ident.to_string())
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

/// `true` when `attrs` includes an attribute whose path's *first*
/// segment is in `gate_cfgs` -- `#[kani::proof]`, `#[kani::
/// proof_for_contract(..)]`, and similarly-namespaced attributes are
/// real, structural verifier entry-point markers, not a name this
/// crate invented: they're only meaningful (indeed only *resolvable at
/// all*) under that verifier's own real attribute-macro namespace,
/// which is exactly what `apply_gate_crates`'s cfg name already
/// identifies. Needed because `amenable_derive::harness!` (the real
/// macro almost every Kani proof harness in `amenable_kani` is
/// declared through) never carries an explicit `#[cfg(kani)]` in its
/// own source text -- the gating is baked into the macro's expansion,
/// invisible to a source-level cfg scan -- but the `#[kani::proof]` it
/// wraps always is.
#[instrument(level = "trace", skip(attrs, gate_cfgs), ret)]
fn has_verifier_attr(attrs: &[syn::Attribute], gate_cfgs: &HashSet<String>) -> bool {
    if gate_cfgs.is_empty() {
        return false;
    }
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .first()
            .is_some_and(|segment| gate_cfgs.contains(&segment.ident.to_string()))
    })
}

/// The items nested inside a macro invocation's trailing brace-
/// delimited block, if its own token stream ends with one --
/// `harness! { kani, NAME, { <real items> } }`'s real shape, but
/// deliberately not tied to `harness!`'s own name: any item-position
/// macro whose last argument is a brace block of real Rust items gets
/// the same treatment, matching this codebase's own "detect the
/// structure, not the name" precedent. `syn` never expands macros, so
/// without this, every item inside is invisible to a syn-only walk --
/// not just the call graph, definitions too.
#[instrument(level = "trace", skip(mac))]
fn trailing_item_block(mac: &syn::Macro) -> Option<Vec<Item>> {
    let last_group = mac
        .tokens
        .clone()
        .into_iter()
        .filter_map(|tree| match tree {
            proc_macro2::TokenTree::Group(group)
                if group.delimiter() == proc_macro2::Delimiter::Brace =>
            {
                Some(group)
            }
            _ => None,
        })
        .last()?;
    let stmts = syn::parse::Parser::parse2(syn::Block::parse_within, last_group.stream()).ok()?;
    let items: Vec<Item> = stmts
        .into_iter()
        .filter_map(|stmt| match stmt {
            syn::Stmt::Item(item) => Some(item),
            _ => None,
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

/// Macro names whose arguments are plain expressions worth looking
/// inside -- `syn` never expands macros, so a call wrapped in one of
/// these (`assert!(Type::method(..))`, the real shape almost every
/// `Ensures`/`Requires` call site in `amenable_kani` actually uses) is
/// otherwise invisible to a syn-only walk entirely. `assert!`/
/// `debug_assert!` take one leading condition expression (an optional
/// message follows); the `_eq`/`_ne` family take two leading value
/// expressions.
const TRANSPARENT_ASSERT_MACROS: &[&str] = &["assert", "debug_assert"];
const TRANSPARENT_COMPARE_MACROS: &[&str] = &[
    "assert_eq",
    "assert_ne",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Collects every unambiguous, name-resolvable call key
/// (`Type::method`/`Trait::method`/`bare_fn`) reached anywhere in one
/// function body -- `receiver.method(..)` calls are not collected at
/// all (see the module doc comment).
struct CallSiteVisitor {
    calls: Vec<String>,
}

impl<'ast> Visit<'ast> for CallSiteVisitor {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Expr::Path(expr_path) = node.func.as_ref() {
            let segments = &expr_path.path.segments;
            match segments.len() {
                1 => self.calls.push(segments[0].ident.to_string()),
                len if len >= 2 => {
                    let type_seg = &segments[len - 2].ident;
                    let method_seg = &segments[len - 1].ident;
                    self.calls.push(format!("{type_seg}::{method_seg}"));
                }
                _ => {}
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    // `syn` never expands macros, and `visit_expr_macro` alone misses a
    // macro invocation written as a whole statement (`assert!(..);`
    // parses as `Stmt::Macro`, not `Stmt::Expr(Expr::Macro(..), ..)`) --
    // `visit_macro` is the one method both forms delegate to, so it's
    // the only hook that reliably sees every macro call site.
    #[instrument(level = "trace", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = node.path.get_ident().map(ToString::to_string);
        let take = if name
            .as_deref()
            .is_some_and(|n| TRANSPARENT_ASSERT_MACROS.contains(&n))
        {
            1
        } else if name
            .as_deref()
            .is_some_and(|n| TRANSPARENT_COMPARE_MACROS.contains(&n))
        {
            2
        } else {
            0
        };
        if take > 0
            && let Ok(args) = syn::parse::Parser::parse2(
                syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                node.tokens.clone(),
            )
        {
            for arg in args.iter().take(take) {
                self.visit_expr(arg);
            }
        }
        syn::visit::visit_macro(self, node);
    }
}

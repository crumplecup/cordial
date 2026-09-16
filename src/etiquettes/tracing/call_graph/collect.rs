use std::collections::HashSet;

use syn::{ImplItem, Item, ItemFn, ItemImpl, ItemMod};
use tracing::instrument;

use crate::PathInclusionFacts;
use crate::config::TracingThresholds;

use super::super::apply::crate_gate_cfgs;
use super::super::scan::{impl_method_local_name, self_type_key, syn_path_label, type_label};

/// One function's identity: which crate it's defined in, and its
/// qualified name the same way [`super::super::scan::scan_rust_source`]
/// records it (module-prefixed, UFCS-qualified --
/// `<Type as Trait>::method` -- for a trait impl method).
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_getters::Getters)]
pub(super) struct FunctionId {
    crate_name: String,
    qualified_name: String,
}

/// One discovered function definition, kept across both passes: first
/// to build the workspace-wide registry, then (once the registry is
/// complete) to resolve its own body's call sites against it.
#[derive(derive_getters::Getters)]
pub(super) struct CollectedFn {
    id: FunctionId,
    /// Registry key(s) a call site could use to reach this function:
    /// `Type::method`/`Trait::method` for impl methods (both, so a
    /// call written through either the type or an in-scope trait
    /// resolves), or the bare name for a free function.
    call_keys: Vec<String>,
    #[getter(copy)]
    ancestor_seed: bool,
    body: Option<syn::Block>,
}

#[instrument(level = "debug", skip(config, path_facts))]
pub(super) fn collect_workspace_functions(
    config: &TracingThresholds,
    path_facts: &PathInclusionFacts,
) -> Vec<CollectedFn> {
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

    collected
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
        // Two renderings of the self type: the generic-preserving key
        // (`RustStdStandard<AtomicI8>`) the method is *recorded* under, so
        // it matches `scan`'s own `never_instrument` lookup; and the bare
        // last-segment label (`RustStdStandard`) a `Type::method(..)` call
        // site actually spells, for `call_keys`.
        let self_ty_bare = type_label(&item_impl.self_ty);
        let self_ty_key = self_type_key(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local =
                impl_method_local_name(&self_ty_key, trait_name.as_deref(), &method.sig.ident);
            let seed = ancestor_seed
                || has_cfg(&method.attrs, self.gate_cfgs)
                || has_verifier_attr(&method.attrs, self.gate_cfgs);
            let mut call_keys = vec![format!("{self_ty_bare}::{}", method.sig.ident)];
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

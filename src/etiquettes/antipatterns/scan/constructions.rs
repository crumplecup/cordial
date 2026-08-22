//! Record which ADT names are constructed in `const`/`static` items vs at runtime.

use std::collections::HashSet;

use syn::visit::Visit;
use syn::{ExprCall, ExprStruct, ImplItemConst, ItemConst, ItemMacro, ItemMod, ItemStatic};

use crate::enricher::is_cfg_test;

use tracing::instrument;

#[instrument(level = "debug", skip(file, const_constructed, runtime_constructed))]
pub(super) fn collect_constructions(
    file: &syn::File,
    const_constructed: &mut HashSet<String>,
    runtime_constructed: &mut HashSet<String>,
) {
    let mut collector = ConstructionCollector {
        in_const: 0,
        const_constructed,
        runtime_constructed,
    };
    collector.visit_file(file);
}

#[instrument(level = "debug", skip(const_constructed, runtime_constructed))]
pub(super) fn types_only_constructed_in_const(
    const_constructed: &HashSet<String>,
    runtime_constructed: &HashSet<String>,
) -> HashSet<String> {
    const_constructed
        .difference(runtime_constructed)
        .cloned()
        .collect()
}

struct ConstructionCollector<'a> {
    in_const: usize,
    const_constructed: &'a mut HashSet<String>,
    runtime_constructed: &'a mut HashSet<String>,
}

impl ConstructionCollector<'_> {
    #[instrument(level = "trace", skip(self, path))]
    fn record_path(&mut self, path: &syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        let target = if self.in_const > 0 {
            &mut self.const_constructed
        } else {
            &mut self.runtime_constructed
        };
        if let Some(last) = segments.last()
            && is_type_ident(last)
        {
            target.insert(last.clone());
        }
        if segments.len() >= 2 {
            let parent = &segments[segments.len() - 2];
            if is_type_ident(parent) {
                target.insert(parent.clone());
            }
        }
    }
}

#[instrument(level = "trace", ret)]
fn is_type_ident(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// `inventory::collect!(TypeName);` declares `TypeName` as the target of
/// the `inventory` crate's static self-registration idiom -- every real
/// instance is built via `inventory::submit! { TypeName { .. } }`
/// elsewhere, which `syn`'s default traversal never sees (a macro's own
/// token stream is opaque unless something re-parses it) and which is
/// often in a different crate entirely (`inventory::collect!` sits beside
/// the type definition; `inventory::submit!` sits beside whatever real
/// item is registering itself, frequently in a sibling crate). `inventory`
/// itself only accepts a const-evaluable value there -- it expands to a
/// `static` -- so a type named here is unconditionally const-placed
/// regardless of which crate submits it.
#[instrument(level = "trace", ret)]
fn inventory_collect_target(mac: &syn::Macro) -> Option<String> {
    let segments: Vec<String> = mac
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    if segments != ["inventory", "collect"] {
        return None;
    }
    syn::parse2::<syn::Path>(mac.tokens.clone())
        .ok()
        .and_then(|path| path.segments.last().map(|segment| segment.ident.to_string()))
}

impl<'ast> Visit<'ast> for ConstructionCollector<'_> {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.in_const += 1;
        syn::visit::visit_item_const(self, node);
        self.in_const -= 1;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.in_const += 1;
        syn::visit::visit_item_static(self, node);
        self.in_const -= 1;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_const(&mut self, node: &'ast ImplItemConst) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.in_const += 1;
        syn::visit::visit_impl_item_const(self, node);
        self.in_const -= 1;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        if let Some(type_name) = inventory_collect_target(&node.mac) {
            self.const_constructed.insert(type_name);
        }
        syn::visit::visit_item_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_struct(&mut self, node: &'ast ExprStruct) {
        self.record_path(&node.path);
        syn::visit::visit_expr_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref() {
            self.record_path(&path.path);
        }
        syn::visit::visit_expr_call(self, node);
    }
}

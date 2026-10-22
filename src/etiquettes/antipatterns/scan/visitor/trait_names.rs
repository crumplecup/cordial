//! Collect local trait names and classify impl targets.

use std::collections::HashSet;

use syn::visit::Visit;
use syn::{ItemImpl, ItemMod, ItemTrait};
use tracing::instrument;

use crate::enricher::is_cfg_test;

/// Trait identifiers defined in this crate's scanned sources.
#[instrument(level = "debug", skip(file))]
pub(in crate::etiquettes::antipatterns::scan) fn collect_local_trait_names(
    file: &syn::File,
) -> HashSet<String> {
    let mut collector = TraitNameCollector {
        names: HashSet::new(),
    };
    collector.visit_file(file);
    collector.names
}

struct TraitNameCollector {
    names: HashSet<String>,
}

impl<'ast> Visit<'ast> for TraitNameCollector {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }
}

#[instrument(level = "trace", skip(node, local_trait_names), ret)]
pub(super) fn is_foreign_trait_impl(node: &ItemImpl, local_trait_names: &HashSet<String>) -> bool {
    let Some((path, _)) = node.trait_.as_ref() else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    !local_trait_names.contains(&segment.ident.to_string())
}

//! Conservative name checks for syntactic error field rewrites.

use std::collections::BTreeSet;
use tracing::instrument;

mod native;

/// Decline a rewrite if a name it relies on can mean something else locally.
/// This does not replace Rust name resolution; it keeps the narrow supported
/// forms from silently accepting a visible shadow or constructor alias.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct NameAudit {
    ambiguous: bool,
    aliased_types: BTreeSet<String>,
    imported_names: BTreeSet<String>,
}

impl NameAudit {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn blocks(&self, type_name: &str) -> bool {
        self.ambiguous
            || self.aliased_types.contains(type_name)
            || self.imported_names.contains(type_name)
    }
}

impl NameAudit {
    #[instrument(level = "debug", skip(self, path))]
    fn check_binding(&mut self, binding: &str, path: &[String]) {
        self.imported_names.insert(binding.to_string());
        if let Some(target) = path.last()
            && target != binding
        {
            self.aliased_types.insert(target.clone());
        }
        if !protected_name(binding) {
            return;
        }
        let known = matches!(path, [root, module, name]
            if (root == "std" || root == "alloc") && module == "string" && name == "String" && binding == "String")
            || matches!(path, [root, module, name]
                if (root == "std" || root == "core") && module == "panic" && name == "Location" && binding == "Location");
        if !known {
            self.ambiguous = true;
        }
    }
}

#[instrument(level = "debug")]
fn protected_name(name: &str) -> bool {
    matches!(
        name,
        "String" | "Location" | "str" | "std" | "core" | "alloc" | "write" | "writeln"
    )
}

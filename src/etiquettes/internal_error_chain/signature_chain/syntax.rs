//! Scope naming and generic binding shared by the signature-chain index.

use std::collections::{BTreeMap, BTreeSet};

use super::Ty;

#[tracing::instrument(level = "debug")]
pub(super) fn scoped(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}

#[tracing::instrument(level = "debug", skip(formal, actual, generics, bindings))]
pub(super) fn bind_generics(
    formal: &super::facts::Type,
    actual: &Ty,
    generics: &BTreeSet<String>,
    bindings: &mut BTreeMap<String, Ty>,
) -> Option<()> {
    let super::facts::Type::Path(path) = formal else {
        return Some(());
    };
    if path.parts().len() == 1 && generics.contains(&path.parts()[0]) {
        bindings.insert(path.parts()[0].clone(), actual.clone());
    }
    Some(())
}

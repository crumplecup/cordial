//! Generated-constructor and storage-independent call-site attribute contracts.
use crate::etiquettes::internal_error_chain::apply::getters::native::{
    known, named, paths, stable_conditional,
};
use crate::loader::DatabaseSource;
use ra_ap_syntax::ast;

#[tracing::instrument(level = "trace", skip(name))]
fn builtin(name: &str) -> bool {
    matches!(
        name,
        "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord" | "Error"
    )
}
#[tracing::instrument(level = "trace", skip(attr))]
pub(super) fn blocking(attr: &ast::Attr) -> bool {
    if named(attr, "derive") {
        return !paths(attr).is_some_and(|paths| {
            paths
                .iter()
                .all(|path| matches!(path.as_slice(), [name] if builtin(name)))
        });
    }
    !["allow", "warn", "deny", "forbid", "cfg", "doc", "repr"]
        .iter()
        .any(|name| named(attr, name))
}
#[tracing::instrument(level = "trace", skip(file, item, attr))]
pub(super) fn stable_file(file: &DatabaseSource<'_>, item: &ast::Struct, attr: &ast::Attr) -> bool {
    if named(attr, "derive") {
        return paths(attr).is_some_and(|paths| {
            paths.iter().all(|path| {
                matches!(path.as_slice(), [name] if builtin(name))
                    || path == &["derive_more", "Display"]
                    || path == &["derive_more", "Error"]
                    || known(file, path, "derive_getters", "Getters")
                    || known(file, path, "derive_more", "Display")
            })
        });
    }
    attr.meta()
        .is_some_and(|meta| stable_conditional(file, item, &meta))
}

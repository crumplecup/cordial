//! Syntactic `Display` evidence for return types the solver cannot judge.
//!
//! Declarations in cfg branches that are inactive in the loaded build have no
//! semantic facts, and derives from crates the manifest does not name do not
//! resolve. Cordial still reads inside those branches, so it falls back to the
//! file's own text: a derived or hand-written `Display` for a local type, a
//! local `Result` alias, and the small well-known list. Anything not positively
//! confirmed stays not displayable.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericArgs, HasName},
};
use tracing::instrument;

use crate::etiquettes::tracing::WELL_KNOWN_DISPLAY_TYPES;
use crate::loader::DatabaseSource;

/// Local types with a `Display` impl and local `Result` aliases in one file.
#[derive(Debug, Default)]
pub(super) struct DisplayFacts {
    displayable: HashSet<String>,
    result_alias_err: HashMap<String, String>,
}

impl DisplayFacts {
    #[instrument(level = "debug", skip(file))]
    pub(super) fn collect(file: &DatabaseSource<'_>) -> Self {
        let mut facts = Self::default();
        for node in file.tree().syntax().descendants() {
            if let Some(item) = ast::Struct::cast(node.clone()) {
                facts.record_derive(&item, item.name());
            } else if let Some(item) = ast::Enum::cast(node.clone()) {
                facts.record_derive(&item, item.name());
            } else if let Some(item) = ast::Impl::cast(node.clone()) {
                facts.record_impl(&item);
            } else if let Some(item) = ast::TypeAlias::cast(node) {
                facts.record_alias(&item);
            }
        }
        facts
    }

    /// Whether `ty` is `Result<_, E>` (directly or through a local alias) with
    /// `E` known displayable.
    #[instrument(level = "trace", skip(self, ty))]
    pub(super) fn err_is_displayable(&self, ty: &ast::Type) -> bool {
        self.err_name(ty).is_some_and(|name| {
            WELL_KNOWN_DISPLAY_TYPES.contains(&name.as_str()) || self.displayable.contains(&name)
        })
    }

    #[instrument(level = "trace", skip(self, item, name))]
    fn record_derive(&mut self, item: &impl HasAttrs, name: Option<ast::Name>) {
        let derives_display = item
            .attrs()
            .filter(|attr| attr.path().is_some_and(|path| path.to_string() == "derive"))
            .flat_map(|attr| attr.syntax().descendants_with_tokens())
            .filter_map(|element| element.into_token())
            .any(|token| token.text() == "Display");
        if let (true, Some(name)) = (derives_display, name) {
            self.displayable.insert(name.text().to_string());
        }
    }

    #[instrument(level = "trace", skip(self, item))]
    fn record_impl(&mut self, item: &ast::Impl) {
        let is_display = item
            .trait_()
            .as_ref()
            .and_then(last_segment_name)
            .is_some_and(|name| name == "Display");
        if let (true, Some(name)) = (
            is_display,
            item.self_ty().as_ref().and_then(last_segment_name),
        ) {
            self.displayable.insert(name);
        }
    }

    #[instrument(level = "trace", skip(self, item))]
    fn record_alias(&mut self, item: &ast::TypeAlias) {
        let (Some(name), Some(ty)) = (item.name(), item.ty()) else {
            return;
        };
        if let Some(err) = direct_result_err_name(&ty) {
            self.result_alias_err.insert(name.text().to_string(), err);
        }
    }

    #[instrument(level = "trace", skip(self, ty))]
    fn err_name(&self, ty: &ast::Type) -> Option<String> {
        match ty {
            ast::Type::PathType(path) => {
                let last = path.path()?.segment()?.name_ref()?.text().to_string();
                if last == "Result" {
                    direct_result_err_name(ty)
                } else {
                    self.result_alias_err.get(&last).cloned()
                }
            }
            ast::Type::RefType(ty) => self.err_name(&ty.ty()?),
            ast::Type::ParenType(ty) => self.err_name(&ty.ty()?),
            _ => None,
        }
    }
}

/// The `E` of a literal `Result<T, E>` with exactly two type arguments.
#[instrument(level = "trace", skip(ty))]
fn direct_result_err_name(ty: &ast::Type) -> Option<String> {
    let ast::Type::PathType(path) = ty else {
        return None;
    };
    let segment = path.path()?.segment()?;
    if segment.name_ref()?.text() != "Result" {
        return None;
    }
    let args: Vec<_> = segment
        .generic_arg_list()?
        .generic_args()
        .filter_map(|arg| match arg {
            ast::GenericArg::TypeArg(arg) => arg.ty(),
            _ => None,
        })
        .collect();
    let [_, err] = args.as_slice() else {
        return None;
    };
    last_segment_name(err)
}

#[instrument(level = "trace", skip(ty))]
fn last_segment_name(ty: &ast::Type) -> Option<String> {
    match ty {
        ast::Type::PathType(path) => Some(path.path()?.segment()?.name_ref()?.text().to_string()),
        ast::Type::RefType(ty) => last_segment_name(&ty.ty()?),
        ast::Type::ParenType(ty) => last_segment_name(&ty.ty()?),
        _ => None,
    }
}

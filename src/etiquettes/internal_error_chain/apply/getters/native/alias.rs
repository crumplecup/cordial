//! Generated alias ownership and exact insertion/removal ranges.
use super::{attrs, start};
use crate::etiquettes::internal_error_chain::apply::{
    Edit,
    getters::{GetterInsertion, alias::marker},
};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericParams, HasModuleItem, HasName, HasVisibility},
};

#[tracing::instrument(level = "trace", skip(file, ty))]
pub(super) fn generated(
    file: &DatabaseSource<'_>,
    ty: &ast::Type,
    owner: &str,
    field: &str,
) -> Option<(String, ast::TypeAlias)> {
    let name = attrs::bare_type(ty)?;
    file.tree().items().find_map(|item| {
        let ast::Item::TypeAlias(item) = item else {
            return None;
        };
        (item.name().is_some_and(|ident| ident.text() == name)
            && marked(&item, owner, field)
            && item.ty().is_some_and(|ty| attrs::static_str(&ty)))
        .then_some((name.clone(), item))
    })
}

#[tracing::instrument(level = "trace", skip(item))]
fn marked(item: &ast::TypeAlias, owner: &str, field: &str) -> bool {
    let marker = marker(owner, field);
    item.doc_comments().any(|doc| doc.text().trim() == marker)
        || item
            .attrs()
            .filter_map(|attr| attrs::doc_value(&attr))
            .any(|doc| doc.trim() == marker)
}

#[tracing::instrument(level = "debug", skip(file, item, field))]
pub(super) fn forward(
    file: &DatabaseSource<'_>,
    item: &ast::Struct,
    field: &ast::RecordField,
) -> Option<GetterInsertion> {
    if item.attrs().any(|attr| {
        attrs::named(&attr, "cfg")
            || (attrs::named(&attr, "cfg_attr")
                && !attr
                    .meta()
                    .is_some_and(|meta| attrs::stable_conditional(file, item, &meta)))
    }) || !attrs::field_stable(file, item, field, false)
    {
        return None;
    }
    let cfg = cfg_attrs(file, field).join("\n");
    Some(GetterInsertion {
        declaration_start: start(item.syntax()),
        field_start: start(field.syntax()),
        cfg: if cfg.is_empty() {
            cfg
        } else {
            format!("{cfg}\n")
        },
    })
}

#[tracing::instrument(level = "debug", skip(file, owner, item, field))]
pub(super) fn reverse(
    file: &DatabaseSource<'_>,
    owner: &ast::Struct,
    item: &ast::TypeAlias,
    field: &ast::RecordField,
) -> Option<Vec<Edit>> {
    if !attrs::field_stable(file, owner, field, true)
        || item.visibility().is_some()
        || item
            .generic_param_list()
            .is_some_and(|list| list.generic_params().next().is_some())
        || item.doc_comments().count()
            + item
                .attrs()
                .filter(|attr| attrs::named(attr, "doc"))
                .count()
            != 1
        || item
            .attrs()
            .any(|attr| !attrs::named(&attr, "doc") && !attrs::named(&attr, "cfg"))
        || !item.ty().is_some_and(|ty| attrs::static_str(&ty))
    {
        return None;
    }
    let getters: Vec<_> = field
        .attrs()
        .filter(|attr| attrs::named(attr, "getter"))
        .collect();
    let [copy] = getters.as_slice() else {
        return None;
    };
    if !attrs::argument(copy, "getter", "copy") || cfg_attrs(file, item) != cfg_attrs(file, field) {
        return None;
    }
    let start = start(item.syntax());
    let end = usize::from(item.syntax().text_range().end());
    let end = end + usize::from(file.source().as_bytes().get(end) == Some(&b'\n'));
    let start = start
        .checked_sub(1)
        .filter(|start| file.source().as_bytes().get(*start) == Some(&b'\n'))
        .unwrap_or(start);
    let range = copy.syntax().text_range();
    let copy_end = usize::from(range.end());
    let copy_end =
        copy_end + file.source()[copy_end..].len() - file.source()[copy_end..].trim_start().len();
    Some(vec![
        Edit {
            start,
            end,
            replacement: String::new(),
        },
        Edit {
            start: usize::from(range.start()),
            end: copy_end,
            replacement: String::new(),
        },
    ])
}

#[tracing::instrument(level = "trace", skip(file, owner))]
fn cfg_attrs(file: &DatabaseSource<'_>, owner: &impl HasAttrs) -> Vec<String> {
    owner
        .attrs()
        .filter(|attr| attrs::named(attr, "cfg"))
        .map(|attr| file.text(attr.syntax()).to_string())
        .collect()
}

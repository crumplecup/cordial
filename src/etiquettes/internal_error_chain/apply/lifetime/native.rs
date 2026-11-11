//! Closed lifetime contracts collected from the loader's canonical Rust tree.
use super::native_syntax::{
    bare_type, generics_empty, one_lifetime, position, range, scalar, str_ref, type_name,
    type_with_lifetime,
};
use super::{
    LifetimeFileFacts, LifetimePlanFacts, LifetimeSiteFacts, native_forwarding as forwarding,
    native_methods as methods,
};
use crate::config::ErrorFieldStrategy;
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericParams, HasModuleItem, HasName},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

struct Index {
    structs: BTreeMap<String, ast::Struct>,
    impls: BTreeMap<String, Vec<ast::Impl>>,
    functions: Vec<ast::Fn>,
}

impl LifetimeFileFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "LifetimeFileFacts")]
    pub(crate) fn collect(file: &DatabaseSource<'_>) -> Self {
        let mut index = Index {
            structs: BTreeMap::new(),
            impls: BTreeMap::new(),
            functions: Vec::new(),
        };
        for item in file.tree().items() {
            match item {
                ast::Item::Struct(item) => {
                    if let Some(name) = item.name() {
                        index.structs.entry(name.text().to_string()).or_insert(item);
                    }
                }
                ast::Item::Impl(item) => {
                    if let Some(name) = item.self_ty().and_then(type_name) {
                        index.impls.entry(name).or_default().push(item);
                    }
                }
                ast::Item::Fn(item) => index.functions.push(item),
                _ => {}
            }
        }
        let mut sites = Vec::new();
        for item in index.structs.values() {
            let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() else {
                continue;
            };
            for field in fields.fields() {
                if let Some(site) = plan(file, &index, item, &field) {
                    sites.push(site);
                }
            }
        }
        Self { sites }
    }
}

#[instrument(level = "debug", skip(file, index, item, field))]
fn plan(
    file: &DatabaseSource<'_>,
    index: &Index,
    item: &ast::Struct,
    field: &ast::RecordField,
) -> Option<LifetimeSiteFacts> {
    let name = item.name()?.text().to_string();
    let field_name = field.name()?.text().to_string();
    let ast::FieldList::RecordFieldList(fields) = item.field_list()? else {
        return None;
    };
    if fields.fields().any(|other| {
        other.name().map(|name| name.text().to_string()) != Some(field_name.clone())
            && other.ty().and_then(scalar).is_none()
    }) {
        return None;
    }
    let ty = field.ty()?;
    let (strategy, lifetime) = if bare_type(&ty, "String") && generics_empty(item) {
        (
            ErrorFieldStrategy::Borrowed,
            methods::constructor_lifetime(
                index.impls.get(&name).map(Vec::as_slice).unwrap_or(&[]),
            )?,
        )
    } else {
        let lifetime = one_lifetime(item)?;
        if !str_ref(&ty, &lifetime) {
            return None;
        }
        (ErrorFieldStrategy::Owned, lifetime)
    };
    if item.doc_comments().next().is_some()
        || item.where_clause().is_some()
        || !item.attrs().all(|attr| {
            !matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
                && attr.path().is_some_and(|path| {
                    path.qualifier().is_none()
                        && path.coloncolon_token().is_none()
                        && path.segment().is_some_and(|segment| {
                            segment.coloncolon_token().is_none()
                                && segment
                                    .name_ref()
                                    .is_some_and(|name| name.text() == "derive")
                        })
                })
        })
    {
        return None;
    }
    let mut impls = Vec::new();
    let mut allowed_type_uses = BTreeSet::new();
    let mut constructors = 0;
    let mut constructor_name = None;
    for implementation in index.impls.get(&name).map(Vec::as_slice).unwrap_or(&[]) {
        if implementation.doc_comments().next().is_some()
            || implementation.attrs().next().is_some()
            || implementation.unsafe_token().is_some()
            || implementation.default_token().is_some()
            || implementation.where_clause().is_some()
            || implementation.assoc_item_list().is_some_and(|items| {
                items.attrs().next().is_some() || items.doc_comments().next().is_some()
            })
        {
            return None;
        }
        let self_ty = implementation.self_ty()?;
        let valid = match strategy {
            ErrorFieldStrategy::Borrowed => generics_empty(implementation),
            ErrorFieldStrategy::Owned => {
                one_lifetime(implementation).as_deref() == Some(&lifetime)
                    && type_with_lifetime(&self_ty, &name, &lifetime)
            }
        };
        if !valid {
            return None;
        }
        allowed_type_uses.insert(position(file.source(), range(self_ty.syntax()).0));
        if let Some(trait_ty) = implementation.trait_() {
            if !methods::allowed_trait(trait_ty) {
                return None;
            }
        } else {
            let members: Vec<_> = implementation.assoc_item_list()?.assoc_items().collect();
            let [ast::AssocItem::Fn(method)] = members.as_slice() else {
                return None;
            };
            if !methods::constructor_signature(method, item, &field_name, &lifetime, strategy) {
                return None;
            }
            constructors += 1;
            constructor_name = method.name().map(|name| name.text().to_string());
        }
        impls.push(methods::impl_facts(implementation, strategy)?);
    }
    if constructors != 1 {
        return None;
    }
    let mut returns = Vec::new();
    for function in &index.functions {
        if let Some(site) = forwarding::forwarding_return(
            file,
            function,
            &name,
            &field_name,
            constructor_name.as_deref()?,
            &lifetime,
            strategy,
        ) {
            allowed_type_uses.insert(site.position);
            returns.push(site);
        }
    }
    Some(LifetimeSiteFacts {
        field_name,
        strategy,
        allowed_type_uses,
        plan: LifetimePlanFacts {
            lifetime,
            type_name: name,
            field_type: range(ty.syntax()),
            struct_ident_end: (strategy == ErrorFieldStrategy::Borrowed)
                .then(|| item.name().map(|name| range(name.syntax()).1))
                .flatten(),
            struct_generics: (strategy == ErrorFieldStrategy::Owned)
                .then(|| item.generic_param_list().map(|list| range(list.syntax())))
                .flatten(),
            impls,
            returns,
            initializers: Default::default(),
        },
    })
}

//! Getter storage contracts from the loader's canonical Rust tree.
use super::{GetterContract, GetterContractFacts, GetterFieldContract};
use crate::loader::DatabaseSource;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};

mod alias;
mod attrs;
pub(super) mod imports;
mod names;

impl GetterContractFacts {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GetterContractFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut fields = Vec::new();
        for item in file.tree().items() {
            let ast::Item::Struct(item) = item else {
                continue;
            };
            if !imports::has_derive(file, &item, "derive_getters", "Getters") {
                continue;
            }
            let Some(owner) = item.name().map(|name| name.text().to_string()) else {
                continue;
            };
            let Some(ast::FieldList::RecordFieldList(list)) = item.field_list() else {
                continue;
            };
            for field in list.fields() {
                let Some(field_name) = field.name().map(|name| name.text().to_string()) else {
                    continue;
                };
                if field_name != "file"
                    || field
                        .attrs()
                        .any(|attr| attrs::argument(&attr, "getter", "skip"))
                {
                    continue;
                }
                let Some(ty) = field.ty() else { continue };
                let storage = if attrs::bare_type(&ty).as_deref() == Some("String") {
                    GetterContract::Owned {
                        insertion: alias::forward(file, &item, &field),
                    }
                } else if let Some((name, alias)) = alias::generated(file, &ty, &owner, &field_name)
                {
                    GetterContract::Generated {
                        alias: name,
                        edits: alias::reverse(file, &item, &alias, &field),
                    }
                } else if attrs::static_str(&ty) {
                    GetterContract::Borrowed
                } else {
                    continue;
                };
                fields.push(GetterFieldContract {
                    owner: owner.clone(),
                    field: field_name,
                    storage,
                });
            }
        }
        Self {
            names: names::collect(file),
            fields,
        }
    }
}

#[tracing::instrument(level = "trace", skip(node))]
fn start(node: &ra_ap_syntax::SyntaxNode) -> usize {
    node.descendants_with_tokens()
        .filter_map(|part| part.into_token())
        .find(|token| !token.kind().is_trivia())
        .map_or(usize::from(node.text_range().start()), |token| {
            usize::from(token.text_range().start())
        })
}

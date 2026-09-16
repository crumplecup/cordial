use std::collections::HashMap;

use syn::{Fields, FieldsNamed, ItemStruct};

use super::FieldMeta;
use crate::etiquettes::derives::syntax::{field_is_exposed, type_label};

use tracing::instrument;

#[instrument(level = "debug", skip(item_struct))]
pub(super) fn collect_struct_fields(
    item_struct: &ItemStruct,
) -> (HashMap<String, FieldMeta>, Vec<String>) {
    let mut fields = HashMap::new();
    let mut exposed_fields = Vec::new();
    match &item_struct.fields {
        Fields::Named(FieldsNamed { named, .. }) => {
            for field in named {
                let Some(ident) = &field.ident else {
                    continue;
                };
                let field_name = ident.to_string();
                let exposed = field_is_exposed(&field.vis);
                fields.insert(
                    field_name.clone(),
                    FieldMeta {
                        is_public: exposed,
                        is_option: type_is_option(&field.ty),
                        boxed_inner_type: boxed_inner_type(&field.ty),
                    },
                );
                if exposed {
                    exposed_fields.push(field_name);
                }
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for (index, field) in fields_unnamed.unnamed.iter().enumerate() {
                if field_is_exposed(&field.vis) {
                    exposed_fields.push(format!("_{index}"));
                }
            }
        }
        Fields::Unit => {}
    }
    (fields, exposed_fields)
}

#[instrument(level = "debug", skip(ty), ret)]
fn type_is_option(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Option")
}

#[instrument(level = "debug", skip(ty), ret)]
fn boxed_inner_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Paren(paren) => boxed_inner_type(&paren.elem),
        syn::Type::Group(group) => boxed_inner_type(&group.elem),
        syn::Type::Path(type_path) => {
            let segment = type_path.path.segments.last()?;
            if segment.ident != "Box" {
                return None;
            }
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                return None;
            };
            args.args.iter().find_map(|arg| {
                let syn::GenericArgument::Type(inner) = arg else {
                    return None;
                };
                Some(type_label(inner))
            })
        }
        _ => None,
    }
}

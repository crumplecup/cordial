use syn::Type;
use tracing::instrument;

use super::Shape;

pub(super) trait HasGenerics {
    fn only_lifetime(&self) -> Option<String>;
}

#[instrument(level = "debug", skip(attrs))]
pub(super) fn allowed_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().all(|attr| {
        if attr.path().is_ident("doc") {
            return true;
        }
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut allowed = true;
        attr.parse_nested_meta(|meta| {
            allowed &= meta.path.get_ident().is_some_and(|name| {
                matches!(
                    name.to_string().as_str(),
                    "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq"
                )
            });
            Ok(())
        })
        .is_ok()
            && allowed
    })
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn storage_shape(ty: &Type) -> Option<(Shape, String)> {
    match ty {
        Type::Path(path) if path.path.is_ident("String") => Some((Shape::String, "str".into())),
        Type::Reference(reference) => {
            if reference.mutability.is_some() {
                return None;
            }
            let inner = type_name(&reference.elem)?;
            Some((
                if inner == "str" {
                    Shape::String
                } else {
                    Shape::Value
                },
                inner,
            ))
        }
        Type::Path(path)
            if path.path.segments.len() == 1 && path.path.segments[0].ident == "Box" =>
        {
            let arg = match &path.path.segments.last()?.arguments {
                syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
                    args.args.first()?
                }
                _ => return None,
            };
            let syn::GenericArgument::Type(inner) = arg else {
                return None;
            };
            Some((Shape::Boxed, type_name(inner)?))
        }
        Type::Path(_) => Some((Shape::Value, type_name(ty)?)),
        _ => None,
    }
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference(reference) => type_name(&reference.elem),
        Type::Path(path) if path.qself.is_none() => {
            if path.path.segments.len() == 1 && path.path.segments[0].ident == "Box" {
                let syn::PathArguments::AngleBracketed(args) =
                    &path.path.segments.last()?.arguments
                else {
                    return None;
                };
                let syn::GenericArgument::Type(inner) = args.args.first()? else {
                    return None;
                };
                return type_name(inner);
            }
            path.path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
        }
        _ => None,
    }
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn is_copy_shape(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Path(path) => path.path.get_ident().is_some_and(|id| {
            matches!(
                id.to_string().as_str(),
                "bool"
                    | "char"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "isize"
            )
        }),
        _ => false,
    }
}

impl HasGenerics for syn::ItemStruct {
    #[instrument(level = "trace", skip(self))]
    fn only_lifetime(&self) -> Option<String> {
        one_lifetime(&self.generics)
    }
}
impl HasGenerics for syn::ItemEnum {
    #[instrument(level = "trace", skip(self))]
    fn only_lifetime(&self) -> Option<String> {
        one_lifetime(&self.generics)
    }
}
#[instrument(level = "debug", skip(generics))]
fn one_lifetime(generics: &syn::Generics) -> Option<String> {
    let mut lifetimes = generics.lifetimes();
    let first = lifetimes.next()?.lifetime.ident.to_string();
    lifetimes.next().is_none().then_some(first)
}

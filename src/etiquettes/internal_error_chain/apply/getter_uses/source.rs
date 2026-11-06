//! Receiver and filename method contracts from the compatibility loader.
use super::{GetterConsumerFacts, MethodContract, ReceiverRecipe};
use crate::loader::SharedParsedFile;
use syn::{Item, Type};
use tracing::instrument;

impl GetterConsumerFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GetterConsumerFacts")]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        Self {
            methods: Self::methods(file),
            uses: super::visit::collect(file),
        }
    }
    #[instrument(level = "trace", skip(file))]
    fn methods(file: &SharedParsedFile) -> Vec<MethodContract> {
        let mut methods = Vec::new();
        for item in &file.syntax().items {
            if let Item::Impl(item) = item {
                if item.trait_.is_some()
                    || !item.generics.params.is_empty()
                    || conditional(&item.attrs)
                {
                    continue;
                }
                let owner = ty(file, &item.self_ty);
                for member in &item.items {
                    if let syn::ImplItem::Fn(method) = member
                        && method.sig.ident == "file"
                        && !conditional(&method.attrs)
                    {
                        methods.push(MethodContract {
                            owner: owner.clone(),
                            has_receiver: method.sig.receiver().is_some(),
                        });
                    }
                }
            }
            if let Item::Struct(item) = item {
                let getters = item
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("derive"))
                    .any(|attr| {
                        let mut known = false;
                        attr.parse_nested_meta(|meta| {
                            known |= super::super::getters::known_derive(
                                file.syntax(),
                                &meta.path,
                                "derive_getters",
                                "Getters",
                            );
                            Ok(())
                        })
                        .is_ok()
                            && known
                    });
                if getters
                    && item.fields.iter().any(|field| {
                        field.ident.as_ref().is_some_and(|name| name == "file")
                            && !field
                                .attrs
                                .iter()
                                .any(|attr| attr.path().is_ident("getter"))
                    })
                {
                    methods.push(MethodContract {
                        owner: ReceiverRecipe::Declaration(item.ident.to_string()),
                        has_receiver: true,
                    });
                }
            }
        }
        methods
    }
}

#[instrument(level = "trace", skip(file, ty))]
pub(super) fn ty(file: &SharedParsedFile, ty: &Type) -> ReceiverRecipe {
    match ty {
        Type::Reference(reference) => self::ty(file, &reference.elem),
        Type::Path(value) if value.qself.is_none() => path(file, &value.path),
        _ => ReceiverRecipe::Unknown,
    }
}

#[instrument(level = "trace", skip(file, path))]
pub(super) fn path(file: &SharedParsedFile, path: &syn::Path) -> ReceiverRecipe {
    let written: Vec<_> = path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect();
    if matches!(written.as_slice(), [root, panic, location] if matches!(root.as_str(), "std" | "core") && panic == "panic" && location == "Location")
    {
        return if unshadowed(file, &written[0]) {
            ReceiverRecipe::Location
        } else {
            ReceiverRecipe::Unknown
        };
    }
    if path
        .segments
        .iter()
        .any(|part| !matches!(part.arguments, syn::PathArguments::None))
    {
        ReceiverRecipe::Unknown
    } else {
        ReceiverRecipe::Path(written)
    }
}

/// Unknown imports and local namespace bindings cannot establish a standard name.
#[instrument(level = "trace", skip(file))]
pub(super) fn unshadowed(file: &SharedParsedFile, name: &str) -> bool {
    !file.syntax().items.iter().any(|item| match item {
        Item::Mod(item) => item.ident == name,
        Item::Struct(item) => item.ident == name,
        Item::Enum(item) => item.ident == name,
        Item::Type(item) => item.ident == name,
        Item::ExternCrate(item) => {
            item.rename
                .as_ref()
                .map(|(_, alias)| alias)
                .unwrap_or(&item.ident)
                == name
        }
        Item::Use(item) => shadows(&item.tree, name),
        _ => false,
    })
}

#[instrument(level = "trace", skip(tree))]
fn shadows(tree: &syn::UseTree, name: &str) -> bool {
    match tree {
        syn::UseTree::Path(path) => shadows(&path.tree, name),
        syn::UseTree::Name(binding) => binding.ident == name,
        syn::UseTree::Rename(binding) => binding.rename == name,
        syn::UseTree::Group(group) => group.items.iter().any(|tree| shadows(tree, name)),
        syn::UseTree::Glob(_) => true,
    }
}

/// Conditional instrumentation preserves a method; conditional existence does not.
#[instrument(level = "trace", skip(attrs))]
fn conditional(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path: Vec<_> = attr.path().segments.iter().map(|part| part.ident.to_string()).collect();
        if !matches!(path.as_slice(), [name] if matches!(name.as_str(), "cfg" | "cfg_attr" | "doc" | "inline" | "track_caller" | "cold" | "must_use" | "deprecated"))
            && path != ["tracing", "instrument"]
        {
            return true;
        }
        attr.path().is_ident("cfg")
            || (attr.path().is_ident("cfg_attr")
                && !attr
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .is_ok_and(|args| {
                        args.len() >= 2
                            && args.iter().skip(1).all(|meta| {
                                let path: Vec<_> = meta
                                    .path()
                                    .segments
                                    .iter()
                                    .map(|part| part.ident.to_string())
                                    .collect();
                                path == ["tracing", "instrument"]
                            })
                    }))
    })
}

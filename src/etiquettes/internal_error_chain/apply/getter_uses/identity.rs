//! Crate-local type identities, imports, and inherent filename signatures.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use syn::{Item, Type};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::inventory::resolution::{absolute_path, module_identities, use_paths};
use super::{Receiver, TypeIdentity};

pub(super) struct Identities {
    declarations: BTreeMap<Vec<String>, Vec<Option<TypeIdentity>>>,
    modules: BTreeMap<PathBuf, Vec<String>>,
    imports: BTreeMap<Vec<String>, Vec<Vec<String>>>,
    methods: BTreeSet<TypeIdentity>,
    associated: BTreeSet<TypeIdentity>,
}

impl Identities {
    #[instrument(level = "debug", skip(files))]
    pub(super) fn collect(files: &[SharedParsedFile]) -> Self {
        let modules = module_identities(files)
            .into_iter()
            .map(|(file, identity)| (file, identity.path().clone()))
            .collect();
        let mut index = Self {
            declarations: BTreeMap::new(),
            modules,
            imports: BTreeMap::new(),
            methods: BTreeSet::new(),
            associated: BTreeSet::new(),
        };
        for file in files {
            index.declarations(file);
        }
        for file in files {
            index.methods(file);
        }
        index
    }

    #[instrument(level = "trace", skip(self, file))]
    fn declarations(&mut self, file: &SharedParsedFile) {
        let Some(module) = self.modules.get(file.path()).cloned() else {
            return;
        };
        for item in &file.syntax().items {
            let name = match item {
                Item::Struct(item) => Some(&item.ident),
                Item::Enum(item) => Some(&item.ident),
                // Aliases occupy the type namespace but do not prove a receiver.
                Item::Type(item) => Some(&item.ident),
                _ => None,
            };
            if let Some(name) = name {
                let mut key = module.clone();
                key.push(name.to_string());
                let identity = (!matches!(item, Item::Type(_))).then(|| TypeIdentity {
                    declaration: file.path().to_path_buf(),
                    name: name.to_string(),
                });
                self.declarations.entry(key).or_default().push(identity);
            }
            if let Item::Use(item) = item {
                let mut imports = Vec::new();
                use_paths(&item.tree, Vec::new(), &mut imports);
                for (alias, written) in imports {
                    let mut key = module.clone();
                    key.push(alias);
                    if item.attrs.iter().any(|attr| !attr.path().is_ident("doc")) {
                        self.declarations.entry(key).or_default().push(None);
                        continue;
                    }
                    if let Some(path) = absolute_path(&module, &written) {
                        self.imports.entry(key.clone()).or_default().push(path);
                    }
                    // Rust 2018 `use module::Type` can start at the crate root.
                    if !matches!(
                        written.first().map(String::as_str),
                        Some("crate" | "self" | "super")
                    ) && !module.is_empty()
                    {
                        self.imports.entry(key).or_default().push(written);
                    }
                }
            }
        }
    }

    #[instrument(level = "trace", skip(self, file))]
    fn methods(&mut self, file: &SharedParsedFile) {
        for item in &file.syntax().items {
            if let Item::Impl(item) = item {
                if item.trait_.is_some()
                    || !item.generics.params.is_empty()
                    || conditional(&item.attrs)
                {
                    continue;
                }
                let Receiver::Declaration(owner) = self.ty(file, &item.self_ty) else {
                    continue;
                };
                for member in &item.items {
                    if let syn::ImplItem::Fn(method) = member
                        && method.sig.ident == "file"
                        && !conditional(&method.attrs)
                    {
                        self.associated.insert(owner.clone());
                        if method.sig.receiver().is_some() {
                            self.methods.insert(owner.clone());
                        }
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
                    let owner = TypeIdentity {
                        declaration: file.path().to_path_buf(),
                        name: item.ident.to_string(),
                    };
                    self.associated.insert(owner.clone());
                    self.methods.insert(owner);
                }
            }
        }
    }

    #[instrument(level = "trace", skip(self, receiver))]
    pub(super) fn method_owner(&self, receiver: Receiver, associated: bool) -> Receiver {
        let owners = if associated {
            &self.associated
        } else {
            &self.methods
        };
        match receiver {
            Receiver::Declaration(ref owner) if !owners.contains(owner) => Receiver::Unknown,
            _ => receiver,
        }
    }

    #[instrument(level = "trace", skip(self, file, ty))]
    pub(super) fn ty(&self, file: &SharedParsedFile, ty: &Type) -> Receiver {
        match ty {
            Type::Reference(reference) => self.ty(file, &reference.elem),
            Type::Path(path) if path.qself.is_none() => self.path(file, &path.path),
            _ => Receiver::Unknown,
        }
    }

    #[instrument(level = "trace", skip(self, file, path))]
    pub(super) fn path(&self, file: &SharedParsedFile, path: &syn::Path) -> Receiver {
        let written: Vec<_> = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        let Some(module) = self.modules.get(file.path()) else {
            return Receiver::Unknown;
        };
        if matches!(written.as_slice(), [root, panic, location] if matches!(root.as_str(), "std" | "core") && panic == "panic" && location == "Location")
        {
            let root = &written[0];
            if !unshadowed(file, root) {
                return Receiver::Unknown;
            }
            return Receiver::Location;
        }
        // A method on Wrapper<u8> does not prove a method on Wrapper<u16>.
        // Until generic instantiations participate in identity, keep them open.
        if path
            .segments
            .iter()
            .any(|part| !matches!(part.arguments, syn::PathArguments::None))
        {
            return Receiver::Unknown;
        }
        let Some(key) = absolute_path(module, &written) else {
            return Receiver::Unknown;
        };
        self.resolve(&key, &mut BTreeSet::new())
            .map_or(Receiver::Unknown, Receiver::Declaration)
    }

    #[instrument(level = "trace", skip(self, key, visited))]
    fn resolve(&self, key: &[String], visited: &mut BTreeSet<Vec<String>>) -> Option<TypeIdentity> {
        if !visited.insert(key.to_vec()) {
            return None;
        }
        let direct = self.declarations.get(key);
        let imported = self.imports.get(key);
        if let Some(targets) = direct {
            return match (targets.as_slice(), imported) {
                ([Some(target)], None) => Some(target.clone()),
                _ => None,
            };
        }
        let mut targets = BTreeSet::new();
        for imported in imported? {
            if let Some(target) = self.resolve(imported, &mut visited.clone()) {
                targets.insert(target);
            }
        }
        let mut targets = targets.into_iter();
        let target = targets.next()?;
        targets.next().is_none().then_some(target)
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

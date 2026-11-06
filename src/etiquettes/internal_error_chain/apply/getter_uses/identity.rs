//! Crate-local type identities, imports, and inherent filename signatures.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::inventory::resolution::{absolute_path, module_identities};
use super::{Receiver, TypeIdentity};

pub(super) struct Identities {
    declarations: BTreeMap<Vec<String>, Vec<Option<TypeIdentity>>>,
    modules: BTreeMap<PathBuf, Vec<String>>,
    imports: BTreeMap<Vec<String>, Vec<Vec<String>>>,
    methods: BTreeSet<TypeIdentity>,
    associated: BTreeSet<TypeIdentity>,
}

impl Identities {
    #[instrument(level = "debug", skip(files, source), err(level = "warn"))]
    pub(super) fn collect(
        files: &[SharedParsedFile],
        source: &crate::ir::SourceErrorApply,
    ) -> crate::CordialResult<Self> {
        let modules = module_identities(files, source)?
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
            index.declarations(file, source)?;
        }
        for file in files {
            for method in &source.getter_uses_for(file)?.methods {
                if let Receiver::Declaration(owner) = index.receiver(file, &method.owner) {
                    index.associated.insert(owner.clone());
                    if method.has_receiver {
                        index.methods.insert(owner);
                    }
                }
            }
        }
        Ok(index)
    }

    #[instrument(level = "trace", skip(self, file, source), err(level = "warn"))]
    fn declarations(
        &mut self,
        file: &SharedParsedFile,
        source: &crate::ir::SourceErrorApply,
    ) -> crate::CordialResult<()> {
        let Some(module) = self.modules.get(file.path()).cloned() else {
            return Ok(());
        };
        let facts = source.declarations_for(file)?;
        let namespace = source.namespace_for(file)?;
        for (name, declared) in facts
            .all()
            .iter()
            .map(|item| (item.name(), true))
            .chain(namespace.aliases().iter().map(|name| (name, false)))
        {
            let mut key = module.clone();
            key.push(name.clone());
            let identity = declared.then(|| TypeIdentity {
                declaration: file.path().to_path_buf(),
                name: name.clone(),
            });
            self.declarations.entry(key).or_default().push(identity);
        }
        for import in namespace.imports() {
            for (alias, written) in import.bindings() {
                let mut key = module.clone();
                key.push(alias.clone());
                if !import.unconditional() {
                    self.declarations.entry(key).or_default().push(None);
                    continue;
                }
                if let Some(path) = absolute_path(&module, written) {
                    self.imports.entry(key.clone()).or_default().push(path);
                }
                // Rust 2018 `use module::Type` can start at the crate root.
                if !matches!(
                    written.first().map(String::as_str),
                    Some("crate" | "self" | "super")
                ) && !module.is_empty()
                {
                    self.imports.entry(key).or_default().push(written.clone());
                }
            }
        }
        Ok(())
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

    #[instrument(level = "trace", skip(self, file, recipe))]
    pub(super) fn receiver(
        &self,
        file: &SharedParsedFile,
        recipe: &super::ReceiverRecipe,
    ) -> Receiver {
        use super::ReceiverRecipe;
        match recipe {
            ReceiverRecipe::Unknown => Receiver::Unknown,
            ReceiverRecipe::Location if self.modules.contains_key(file.path()) => {
                Receiver::Location
            }
            ReceiverRecipe::Location => Receiver::Unknown,
            ReceiverRecipe::Declaration(name) => Receiver::Declaration(TypeIdentity {
                declaration: file.path().to_path_buf(),
                name: name.clone(),
            }),
            ReceiverRecipe::Method {
                receiver,
                associated,
            } => self.method_owner(self.receiver(file, receiver), *associated),
            ReceiverRecipe::Path(written) => {
                let Some(module) = self.modules.get(file.path()) else {
                    return Receiver::Unknown;
                };
                let Some(key) = absolute_path(module, written) else {
                    return Receiver::Unknown;
                };
                self.resolve(&key, &mut BTreeSet::new())
                    .map_or(Receiver::Unknown, Receiver::Declaration)
            }
        }
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

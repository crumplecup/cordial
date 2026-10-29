//! Conservative crate-local identities for declarations referenced by apply.
//!
//! The index connects facts collected from separate shared source trees. It
//! only resolves a path when the spelling and module location identify one
//! private top-level declaration. Unknown imports and ambiguous names stay
//! unresolved for the conversion planner.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use syn::{Item, Visibility};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::edges::{ErrorEdgeTarget, ErrorEdgeTargetKind};
use super::helpers::derive_is_builtin;
use super::syntax_facts::FileSyntaxFacts;

mod ir;
mod lookup;
mod paths;

use paths::{active_modules, use_paths};

pub(in crate::etiquettes::internal_error_chain::apply) struct CrateTypeIndex {
    declarations: HashMap<Vec<String>, Vec<Option<ErrorEdgeTarget>>>,
    local: HashMap<(PathBuf, String), Vec<Option<ErrorEdgeTarget>>>,
    active_modules: HashMap<PathBuf, Vec<String>>,
    imports: HashMap<PathBuf, Vec<(String, Vec<String>)>>,
    file_syntax: HashMap<PathBuf, FileSyntaxFacts>,
    incoming_edges: Vec<(String, ErrorEdgeTarget)>,
}

impl CrateTypeIndex {
    #[instrument(level = "debug", skip(files))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn new(
        files: &[SharedParsedFile],
    ) -> Self {
        let getters = super::super::getters::collect_getter_storage(files);
        let mut declarations: HashMap<Vec<String>, Vec<Option<ErrorEdgeTarget>>> = HashMap::new();
        let mut local: HashMap<(PathBuf, String), Vec<Option<ErrorEdgeTarget>>> = HashMap::new();
        let active_modules = active_modules(files);
        let mut imports = HashMap::new();
        let mut tracked_fields = BTreeSet::new();
        for file in files {
            let mut file_imports = Vec::new();
            for item in &file.syntax().items {
                match item {
                    Item::Struct(declaration) => tracked_fields.extend(
                        declaration
                            .fields
                            .iter()
                            .filter_map(|field| field.ident.as_ref().map(ToString::to_string)),
                    ),
                    Item::Enum(declaration) => {
                        for variant in &declaration.variants {
                            tracked_fields.extend(
                                variant.fields.iter().filter_map(|field| {
                                    field.ident.as_ref().map(ToString::to_string)
                                }),
                            );
                        }
                    }
                    _ => {}
                }
                if let Item::Use(item) = item {
                    use_paths(&item.tree, Vec::new(), &mut file_imports);
                }
                let (name, visibility, attrs, kind, location) = match item {
                    Item::Struct(item) => (
                        &item.ident,
                        &item.vis,
                        &item.attrs,
                        ErrorEdgeTargetKind::Struct,
                        item.ident.span().start(),
                    ),
                    Item::Enum(item) => (
                        &item.ident,
                        &item.vis,
                        &item.attrs,
                        ErrorEdgeTargetKind::Enum,
                        item.ident.span().start(),
                    ),
                    _ => continue,
                };
                let target = (!matches!(visibility, Visibility::Public(_))
                    && attrs.iter().all(derive_is_builtin))
                .then(|| {
                    ErrorEdgeTarget::from_shared_file(file, location.line, location.column, kind)
                });
                local
                    .entry((file.path().to_path_buf(), name.to_string()))
                    .or_default()
                    .push(target.clone());
                if let Some(module) = active_modules.get(file.path()) {
                    let mut key = module.clone();
                    key.push(name.to_string());
                    declarations.entry(key).or_default().push(target);
                }
            }
            imports.insert(file.path().to_path_buf(), file_imports);
        }
        let tracked_names: BTreeSet<_> = local.keys().map(|(_, name)| name.clone()).collect();
        let mut index = Self {
            declarations,
            local,
            active_modules,
            imports,
            file_syntax: files
                .iter()
                .map(|file| {
                    (
                        file.path().to_path_buf(),
                        FileSyntaxFacts::collect(
                            file,
                            &tracked_names,
                            &tracked_fields,
                            getters.get(file.path()),
                        ),
                    )
                })
                .collect(),
            incoming_edges: Vec::new(),
        };
        index.incoming_edges = index.collect_incoming_edges(files);
        index
    }

    #[instrument(level = "debug", skip(self, files))]
    fn collect_incoming_edges(&self, files: &[SharedParsedFile]) -> Vec<(String, ErrorEdgeTarget)> {
        let mut incoming = Vec::new();
        for file in files {
            if let Some(facts) = self.graph_declarations(file) {
                incoming.extend(facts.resolved_incoming_edges(|path| self.resolve(file, path)));
            }
        }
        incoming
    }

    #[instrument(level = "trace", skip(self, names))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn anchors_for(
        &self,
        names: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        self.file_syntax
            .values()
            .flat_map(|facts| facts.lifetimes().anchors_for(names))
            .collect()
    }
}

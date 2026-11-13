//! A cfg context as crates of its own in the session database.
//!
//! The context pass used to change every local crate's cfg input and put it back,
//! and each change made every result Salsa had memoised stale. rust-analyzer
//! already supports the same root file in more than one crate (it keys such
//! crates by their data and cfg), and every query it answers is keyed by crate.
//! So a context is added as a copy of each local crate with its cfg changed, and
//! the ordinary crates are never touched: what they computed stays valid, and
//! the context's results coexist with theirs.

use std::collections::BTreeSet;

use ra_ap_hir::{AssocItem, Crate, Function, HasSource, ModuleDef, attach_db};
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{Crate as InputCrate, all_crates, set_all_crates_with_durability};
use salsa::Durability;
use tracing::instrument;

use super::SourceDatabase;
use super::contexts::LocalCfgs;
use crate::{CordialResult, FunctionLocation, SemanticContextKey};

/// The crates a context added: each ordinary local crate with the crate that
/// holds the same sources under the context's cfg.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct ContextCrates {
    context: SemanticContextKey,
    #[getter(skip)]
    crates: Vec<(InputCrate, InputCrate)>,
}

impl ContextCrates {
    /// The context these crates analyse the sources under.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn shadows(&self) -> impl Iterator<Item = InputCrate> + '_ {
        self.crates.iter().map(|(_, shadow)| *shadow)
    }
}

impl SourceDatabase {
    /// Add `context` as crates beside the ordinary ones, once: asking again for a
    /// context already added returns it. The ordinary crates' inputs are not
    /// written, so nothing computed for them is invalidated.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn add_context_crates(
        &mut self,
        context: &SemanticContextKey,
    ) -> CordialResult<ContextCrates> {
        if let Some(existing) = self
            .context_crates
            .iter()
            .find(|added| added.context() == context)
        {
            return Ok(existing.clone());
        }
        let ordinary = LocalCfgs::capture_excluding(&self.db, &self.shadow_crates());
        let mut shadows: Vec<(InputCrate, InputCrate)> = Vec::new();
        for (krate, options) in ordinary.cfgs() {
            let mut changed = options.clone();
            changed.apply_diff(LocalCfgs::diff_of(context));
            let mut data = krate.data(&self.db).clone();
            // A dependency on another local crate follows to that crate's copy,
            // so a test binary built for the context sees the library built for it.
            for dependency in &mut data.dependencies {
                if let Some((_, copy)) = shadows
                    .iter()
                    .find(|(original, _)| *original == dependency.crate_id)
                {
                    dependency.crate_id = *copy;
                }
            }
            let copy = InputCrate::builder(
                data,
                krate.extra_data(&self.db).clone(),
                krate.workspace_data(&self.db).clone(),
                changed,
                krate.env(&self.db).clone(),
            )
            .durability(Durability::MEDIUM)
            .new(&self.db);
            shadows.push((*krate, copy));
        }
        let mut registered: Vec<InputCrate> = all_crates(&self.db).iter().copied().collect();
        registered.extend(shadows.iter().map(|(_, copy)| *copy));
        set_all_crates_with_durability(&mut self.db, registered, Durability::MEDIUM);
        let added = ContextCrates {
            context: context.clone(),
            crates: shadows,
        };
        self.context_crates.push(added.clone());
        Ok(added)
    }

    /// The functions declared in the ordinary build's local crates.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn ordinary_functions(&self) -> BTreeSet<FunctionLocation> {
        let shadows = self.shadow_crates();
        self.locations(|krate| !shadows.contains(&krate.base()))
    }

    /// The functions declared in the crates `context` added: those active under
    /// its cfg.
    #[instrument(level = "debug", skip(self, context))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn context_functions(&self, context: &ContextCrates) -> BTreeSet<FunctionLocation> {
        let mine: BTreeSet<InputCrate> = context.shadows().collect();
        self.locations(|krate| mine.contains(&krate.base()))
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn shadow_crates(&self) -> BTreeSet<InputCrate> {
        self.context_crates
            .iter()
            .flat_map(ContextCrates::shadows)
            .collect()
    }

    /// Locations of the functions in the local crates `keep` accepts.
    #[instrument(level = "debug", skip(self, keep))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn locations(&self, keep: impl Fn(&Crate) -> bool) -> BTreeSet<FunctionLocation> {
        attach_db(&self.db, || {
            let mut found = BTreeSet::new();
            for krate in Crate::all(&self.db) {
                if !krate.origin(&self.db).is_local() || !keep(&krate) {
                    continue;
                }
                for function in functions_of(&self.db, krate) {
                    let source = function.source(&self.db);
                    let Some(source) = source else { continue };
                    if source.file_id.is_macro() {
                        continue;
                    }
                    let file = source.file_id.original_file(&self.db).file_id(&self.db);
                    let Some(path) = self.vfs.file_path(file).as_path() else {
                        continue;
                    };
                    let start = u32::from(
                        ra_ap_syntax::AstNode::syntax(&source.value)
                            .text_range()
                            .start(),
                    );
                    found.insert(FunctionLocation::new(
                        AsRef::<std::path::Path>::as_ref(path).to_path_buf(),
                        start,
                    ));
                }
            }
            found
        })
    }
}

/// Every function a crate declares: free functions, trait methods and the
/// methods of each impl.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn functions_of(db: &RootDatabase, krate: Crate) -> Vec<Function> {
    let mut functions = Vec::new();
    for module in krate.modules(db) {
        for declaration in module.declarations(db) {
            match declaration {
                ModuleDef::Function(function) => functions.push(function),
                ModuleDef::Trait(declared) => functions.extend(
                    declared
                        .items(db)
                        .into_iter()
                        .filter_map(|item| match item {
                            AssocItem::Function(function) => Some(function),
                            _ => None,
                        }),
                ),
                _ => {}
            }
        }
        for implementation in module.impl_defs(db) {
            functions.extend(
                implementation
                    .items(db)
                    .into_iter()
                    .filter_map(|item| match item {
                        AssocItem::Function(function) => Some(function),
                        _ => None,
                    }),
            );
        }
    }
    functions
}

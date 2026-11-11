//! Combine compatible context requests, and check the combination.
//!
//! Every context costs a rebuild of the workspace crates' definition maps, so
//! requests that can hold at once share one. Two requests can share a context
//! when neither turns off what the other turns on. That is necessary, not
//! sufficient: exclusive alternatives (`cfg(kani)` against `cfg(creusot)`) can
//! be compatible and still define the same item twice. So a merged context is
//! checked once it is entered, and one that declares a name twice is dissolved
//! back into its members.

use crate::SemanticContextKey;
use ra_ap_hir::{Crate, ModuleDef, attach_db};
use ra_ap_ide_db::RootDatabase;
use std::collections::BTreeSet;
use tracing::instrument;

/// A context waiting to be analysed, and the requests it stands for.
#[derive(Debug, Clone, derive_getters::Getters)]
pub(super) struct Pending {
    key: SemanticContextKey,
    members: Vec<SemanticContextKey>,
}

impl Pending {
    /// A request that stands alone.
    #[instrument(level = "debug", skip(key))]
    pub(super) fn single(key: SemanticContextKey) -> Self {
        Self {
            members: vec![key.clone()],
            key,
        }
    }

    /// Whether this combines more than one request.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn is_merged(&self) -> bool {
        self.members.len() > 1
    }

    /// The two halves of a combination that did not hold, each combining its own
    /// members. Splitting in halves converges on the conflicting requests
    /// without separating the ones that were never in conflict.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn halves(&self) -> Vec<Pending> {
        let middle = self.members.len() / 2;
        [&self.members[..middle], &self.members[middle..]]
            .into_iter()
            .filter_map(|members| {
                members
                    .iter()
                    .fold(None::<Pending>, |combined, member| match combined {
                        None => Some(Pending::single(member.clone())),
                        Some(mut pending) => {
                            pending.key = union(&pending.key, member).unwrap_or(pending.key);
                            pending.members.push(member.clone());
                            Some(pending)
                        }
                    })
            })
            .collect()
    }
}

/// `a` and `b` together, or `None` when one turns off what the other turns on.
#[instrument(level = "trace", skip(a, b))]
fn union(a: &SemanticContextKey, b: &SemanticContextKey) -> Option<SemanticContextKey> {
    let clash = |on: &[String], off: &[String]| on.iter().any(|atom| off.contains(atom));
    if clash(a.enabled(), b.disabled()) || clash(b.enabled(), a.disabled()) {
        return None;
    }
    Some(SemanticContextKey::new(
        a.enabled().iter().chain(b.enabled()).cloned(),
        a.disabled().iter().chain(b.disabled()).cloned(),
    ))
}

/// Greedily combine `keys` into as few compatible contexts as possible.
#[instrument(level = "debug", skip(keys))]
pub(super) fn group(keys: Vec<SemanticContextKey>) -> Vec<Pending> {
    let mut groups: Vec<Pending> = Vec::new();
    for key in keys {
        let home = groups
            .iter_mut()
            .find_map(|group| union(&group.key, &key).map(|merged| (group, merged)));
        match home {
            Some((group, merged)) => {
                group.key = merged;
                group.members.push(key);
            }
            None => groups.push(Pending::single(key)),
        }
    }
    groups
}

/// Whether any module of a local crate declares one name twice in a namespace,
/// which no real build can: it means the active cfg set combines alternatives
/// that exclude each other.
#[instrument(level = "debug", skip(db))]
pub(super) fn declares_a_name_twice(db: &RootDatabase) -> bool {
    attach_db(db, || {
        Crate::all(db)
            .into_iter()
            .filter(|krate| krate.origin(db).is_local())
            .flat_map(|krate| krate.modules(db))
            .any(|module| {
                let mut seen: BTreeSet<(Namespace, String)> = BTreeSet::new();
                module.declarations(db).into_iter().any(|definition| {
                    let (Some(namespace), Some(name)) =
                        (namespace(&definition), definition.name(db))
                    else {
                        return false;
                    };
                    !seen.insert((namespace, name.as_str().to_owned()))
                })
            })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Namespace {
    Types,
    Values,
    Macros,
}

#[instrument(level = "debug", skip(definition))]
fn namespace(definition: &ModuleDef) -> Option<Namespace> {
    match definition {
        ModuleDef::Function(_) | ModuleDef::Const(_) | ModuleDef::Static(_) => {
            Some(Namespace::Values)
        }
        ModuleDef::Module(_)
        | ModuleDef::Adt(_)
        | ModuleDef::Trait(_)
        | ModuleDef::TypeAlias(_) => Some(Namespace::Types),
        ModuleDef::Macro(_) => Some(Namespace::Macros),
        _ => None,
    }
}

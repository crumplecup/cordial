//! Collect getter contracts once, before serializing conversion evidence.

use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::names::Names;
use super::{GetterFiles, GetterStorageFacts};

#[instrument(level = "debug", skip(files, uses, source), err(level = "warn"))]
pub(in crate::etiquettes::internal_error_chain::apply) fn collect_getter_storage(
    files: &[crate::FileSnapshot],
    uses: &[super::super::getter_uses::GetterUseFact],
    source: &crate::ir::SourceErrorApply,
) -> crate::CordialResult<GetterFiles> {
    let mut names = Names::default();
    for file in files {
        names.extend(&source.getters_for(file)?.names);
    }
    let mut facts = GetterFiles::new();
    for file in files {
        let fields = facts.entry(file.path().to_path_buf()).or_default();
        for contract in &source.getters_for(file)?.fields {
            let super::GetterFieldContract {
                owner,
                field,
                storage,
            } = contract;
            let blocked = uses.iter().any(|usage| usage.blocks(file.path(), owner));
            let storage = match storage {
                super::GetterContract::Owned { insertion } => {
                    let alias = names.reserve(owner, field);
                    GetterStorageFacts {
                        edits: insertion
                            .as_ref()
                            .filter(|_| !blocked)
                            .map(|insertion| insertion.edits(owner, field, &alias)),
                        alias,
                        target: ErrorFieldStrategy::Borrowed,
                    }
                }
                super::GetterContract::Generated { alias, edits } => GetterStorageFacts {
                    alias: alias.clone(),
                    edits: edits
                        .clone()
                        .filter(|_| !blocked && names.count(alias) == 2),
                    target: ErrorFieldStrategy::Owned,
                },
                super::GetterContract::Borrowed => GetterStorageFacts {
                    alias: String::new(),
                    edits: None,
                    target: ErrorFieldStrategy::Owned,
                },
            };
            fields.insert((owner.clone(), field.clone()), storage);
        }
    }
    Ok(facts)
}

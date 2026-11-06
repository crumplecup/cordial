//! Shared declaration identity and field names for crate graph joins.

use super::{DeclarationKind, GraphDeclarationFacts, Visibility};
use crate::etiquettes::internal_error_chain::apply::inventory::{
    ErrorEdgeTarget, ErrorEdgeTargetKind,
};
use crate::loader::SourceSnapshot;
use std::collections::BTreeSet;

impl GraphDeclarationFacts {
    #[tracing::instrument(level = "trace", skip(self, file))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn resolution_targets(
        &self,
        file: &impl SourceSnapshot,
    ) -> Vec<(String, Option<ErrorEdgeTarget>)> {
        self.declarations
            .iter()
            .map(|item| {
                let target =
                    (item.visibility != Visibility::Public && item.linkable_attrs).then(|| {
                        ErrorEdgeTarget::from_snapshot(
                            file,
                            item.line,
                            item.column,
                            match item.kind {
                                DeclarationKind::Struct => ErrorEdgeTargetKind::Struct,
                                DeclarationKind::Enum => ErrorEdgeTargetKind::Enum,
                            },
                        )
                    });
                (item.name.clone(), target)
            })
            .collect()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn field_names(
        &self,
    ) -> BTreeSet<String> {
        self.declarations
            .iter()
            .flat_map(|item| {
                item.fields.iter().chain(
                    item.variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter()),
                )
            })
            .filter_map(|field| field.name.clone())
            .collect()
    }
}

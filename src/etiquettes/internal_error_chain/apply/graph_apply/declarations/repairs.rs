//! Match completed type edits to compliance sites collected in the shared IR.

use std::path::Path;

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::etiquettes::internal_error_chain::InternalErrorComplianceId;
use crate::loader::{SourceEditView, module_path_from_src_file};

use super::super::super::{Edit, ErrorFieldApplyRepair};
use super::{GraphDeclarationFacts, GraphFields};

impl GraphDeclarationFacts {
    #[instrument(level = "debug", skip(self, file, edits, strategy))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn repairs(
        &self,
        crate_name: &str,
        crate_root: &Path,
        file: &SourceEditView,
        edits: &[Edit],
        strategy: ErrorFieldStrategy,
    ) -> Vec<ErrorFieldApplyRepair> {
        let prefix = module_path_from_src_file(&crate_root.join("src"), file.path());
        let mut repairs = Vec::new();
        for item in &self.declarations {
            let context = prefix
                .iter()
                .map(String::as_str)
                .chain([item.name.as_str()])
                .collect::<Vec<_>>()
                .join("::");
            let groups = std::iter::once((None, &item.fields)).chain(
                item.variants
                    .iter()
                    .map(|variant| (Some(variant.name.as_str()), &variant.fields)),
            );
            for (variant, fields) in groups {
                let fields = match fields {
                    GraphFields::Named(fields) | GraphFields::Unnamed(fields) => fields.as_slice(),
                    GraphFields::Unit => continue,
                };
                for (index, field) in fields.iter().enumerate() {
                    let Some(edit) = edits
                        .iter()
                        .find(|edit| field.type_range == Some((edit.start, edit.end)))
                    else {
                        continue;
                    };
                    let name = field.name.clone().unwrap_or_else(|| format!("_{index}"));
                    let (rule_id, context, line) = if name == "kind" && variant.is_none() {
                        (
                            InternalErrorComplianceId::ArchKindBox001,
                            context.clone(),
                            item.declaration_line,
                        )
                    } else if strategy == ErrorFieldStrategy::Borrowed
                        && ((edit.replacement.starts_with('&')
                            && (name != "file" || edit.replacement == "&'static str"))
                            || field.getter_storage.as_ref().is_some_and(|getter| {
                                getter
                                    .plan(strategy)
                                    .is_some_and(|(replacement, _)| replacement == edit.replacement)
                            }))
                    {
                        let context = match variant {
                            Some(variant) => format!("{context}::{variant}::{name}"),
                            None => format!("{context}::{name}"),
                        };
                        (
                            InternalErrorComplianceId::FieldStrategy001,
                            context,
                            field.type_line,
                        )
                    } else {
                        continue;
                    };
                    repairs.push(ErrorFieldApplyRepair {
                        crate_name: crate_name.to_string(),
                        file: file.path().to_path_buf(),
                        context,
                        line: line as u32,
                        rule_id,
                    });
                }
            }
        }
        repairs
    }
}

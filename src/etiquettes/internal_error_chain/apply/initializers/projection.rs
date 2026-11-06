//! Candidate-independent constructor proofs; selection never visits source syntax.
use super::replacements::InitializerExpressionFact;
use super::{FieldKind, InitializerFacts};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tracing::instrument;

#[derive(derive_new::new)]
pub(crate) struct InitializerBatchInput<'a> {
    type_name: &'a str,
    field_name: &'a str,
    kind: FieldKind,
    lifetime: Option<&'a str>,
    pending_lifetime: bool,
    cfg_attrs: &'a [String],
}

/// Every record initializer in a file, bound to its containing source snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(crate) struct InitializerFileFacts {
    records: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(super) struct Record {
    path: Vec<String>,
    bare: bool,
    owner: Option<String>,
    impl_lifetime: Option<String>,
    lifetime_context: bool,
    rest: bool,
    fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(super) struct Field {
    name: String,
    cfg: Option<Vec<String>>,
    file: Vec<InitializerExpressionFact>,
    string: Option<StringProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(super) struct StringProof {
    fact: InitializerExpressionFact,
    origin: StringOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum StringOrigin {
    Static,
    FileConstant(String),
    Lifetime(String),
}

impl InitializerFileFacts {
    #[instrument(level = "trace", skip(self, inputs, static_strings))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn select(
        &self,
        inputs: &[InitializerBatchInput<'_>],
        static_strings: &BTreeSet<String>,
    ) -> Vec<InitializerFacts> {
        inputs
            .iter()
            .map(|input| self.for_input(input, static_strings))
            .collect()
    }

    #[instrument(level = "trace", skip(self, input, static_strings))]
    fn for_input(
        &self,
        input: &InitializerBatchInput<'_>,
        static_strings: &BTreeSet<String>,
    ) -> InitializerFacts {
        let mut facts = InitializerFacts::default();
        for record in &self.records {
            let target = record.bare
                && (record
                    .path
                    .first()
                    .is_some_and(|name| name == input.type_name)
                    || (record.path.first().is_some_and(|name| name == "Self")
                        && record.owner.as_deref() == Some(input.type_name)));
            if !target {
                facts.unresolved |= record
                    .path
                    .last()
                    .is_some_and(|name| name == input.type_name);
                continue;
            }
            facts.found += 1;
            let field = record
                .fields
                .iter()
                .find(|field| field.name == input.field_name);
            let Some(field) =
                field.filter(|field| !record.rest && field.cfg.as_deref() == Some(input.cfg_attrs))
            else {
                facts.unresolved = true;
                continue;
            };
            match input.kind {
                FieldKind::CallSiteFile if !field.file.is_empty() => {
                    facts.sites.extend(field.file.iter().cloned().map(Some));
                }
                FieldKind::StaticString => {
                    let proof = field.string.as_ref().filter(|proof| match &proof.origin {
                        StringOrigin::Static => true,
                        StringOrigin::FileConstant(name) => static_strings.contains(name),
                        StringOrigin::Lifetime(lifetime) => {
                            input.lifetime == Some(lifetime.as_str())
                                && record.lifetime_context
                                && record.owner.as_deref() == Some(input.type_name)
                                && (record.impl_lifetime.as_ref() == Some(lifetime)
                                    || input.pending_lifetime)
                        }
                    });
                    facts.sites.push(proof.map(|proof| proof.fact.clone()));
                }
                _ => facts.sites.push(None),
            }
        }
        facts
    }
}

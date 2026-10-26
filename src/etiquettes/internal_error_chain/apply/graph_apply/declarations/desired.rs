//! Compute field type replacements from collected declaration evidence.

use std::collections::BTreeSet;

use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::GraphField;

impl GraphField {
    #[instrument(level = "trace", skip(self, strategy, changing_generics))]
    pub(in crate::etiquettes::internal_error_chain::apply::graph_apply) fn desired_type(
        &self,
        strategy: ErrorFieldStrategy,
        owner_lifetime: Option<&str>,
        field_name: Option<&str>,
        graph_lifetime: Option<&str>,
        changing_generics: &BTreeSet<String>,
    ) -> Option<String> {
        let (_, inner) = self.storage()?;
        match strategy {
            ErrorFieldStrategy::Borrowed if !self.is_reference() => {
                let lifetime = if field_name == Some("file") {
                    "static"
                } else {
                    owner_lifetime.or(graph_lifetime)?
                };
                let referent = self.borrowed_referent.as_deref()?;
                let referent = if changing_generics.contains(inner) {
                    format!("{referent}<'{lifetime}>")
                } else {
                    referent.to_string()
                };
                Some(format!("&'{lifetime} {referent}"))
            }
            ErrorFieldStrategy::Owned if self.is_reference() && inner == "str" => {
                Some("String".to_string())
            }
            ErrorFieldStrategy::Owned if self.is_reference() => {
                let referent = if changing_generics.contains(inner) {
                    let (lifetime, stripped) = self.stripped_lifetime.as_ref()?;
                    if graph_lifetime != Some(lifetime.as_str()) {
                        return None;
                    }
                    stripped.as_str()
                } else {
                    self.owned_referent.as_deref()?
                };
                if field_name == Some("kind") {
                    Some(format!("Box<{referent}>"))
                } else {
                    Some(referent.to_string())
                }
            }
            _ => None,
        }
    }
}

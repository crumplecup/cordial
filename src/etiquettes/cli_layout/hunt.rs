//! Select nested clap types from shared CLI-layout evidence.
use crate::ir::{TypeRec, VariantShape};
use std::collections::BTreeSet;
use tracing::instrument;
#[instrument(level = "debug", skip(item, clap_idents))]
pub(super) fn nested_clap_types(
    item: &TypeRec,
    clap_idents: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut consider = |idents: &[String]| {
        for ident in idents {
            if clap_idents.contains(ident) && ident != item.ident() {
                out.insert(ident.clone());
            }
        }
    };
    for idents in item.fields().values() {
        consider(idents);
    }
    for variant in item.variants().values() {
        match variant {
            VariantShape::Named(fields) => {
                for idents in fields.values() {
                    consider(idents);
                }
            }
            VariantShape::Unnamed(fields) => {
                for idents in fields {
                    consider(idents);
                }
            }
            VariantShape::Unit => {}
        }
    }
    out
}

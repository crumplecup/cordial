//! Syntactic input and output paths for `From` implementations.

use syn::{GenericArgument, ItemImpl, PathArguments};
use tracing::instrument;

use super::helpers::plain_type_path;

/// Type paths written in a `From<Input> for Output` implementation.
///
/// These paths have not undergone name resolution. An absent path means the
/// signature uses a shape such as a reference or associated type that this
/// inventory does not yet describe.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub struct ErrorFromEvidence {
    trait_path: Vec<String>,
    input_type_path: Option<Vec<String>>,
    output_type_path: Option<Vec<String>>,
}

impl ErrorFromEvidence {
    #[instrument(level = "debug", skip(item))]
    pub(super) fn from_impl(item: &ItemImpl) -> Option<Self> {
        let (trait_path, _) = item.trait_.as_ref()?;
        let from = trait_path.segments.last()?;
        if from.ident != "From" {
            return None;
        }
        let input_type_path = match &from.arguments {
            PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
                match args.args.first()? {
                    GenericArgument::Type(ty) => plain_type_path(ty),
                    _ => None,
                }
            }
            _ => None,
        };
        Some(Self {
            trait_path: trait_path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect(),
            input_type_path,
            output_type_path: plain_type_path(&item.self_ty),
        })
    }
}

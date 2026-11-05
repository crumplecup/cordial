//! Parser-independent field storage contracts retained for policy assessment.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tracing::instrument;

mod collect;
mod native;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct FieldType {
    label: String,
    #[getter(copy)]
    static_str: bool,
    #[getter(copy)]
    borrowed_string: bool,
    copy: CopyRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum CopyRequirement {
    Always,
    Never,
    All(Vec<Self>),
    Path { name: String, arguments: Arguments },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Arguments {
    None,
    Angle(Vec<CopyRequirement>),
    Unsupported,
}

impl FieldType {
    #[instrument(level = "trace", skip(self, local_copy), ret)]
    pub(crate) fn is_copy(&self, local_copy: &BTreeSet<String>) -> bool {
        self.copy.satisfied(local_copy)
    }
}

impl CopyRequirement {
    #[instrument(level = "trace", skip(self, local_copy), ret)]
    fn satisfied(&self, local_copy: &BTreeSet<String>) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::All(types) => types.iter().all(|ty| ty.satisfied(local_copy)),
            Self::Path { name, arguments } => {
                if matches!(
                    name.as_str(),
                    "bool"
                        | "char"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "f32"
                        | "f64"
                ) {
                    return true;
                }
                match arguments {
                    Arguments::None => local_copy.contains(name),
                    Arguments::Angle(types) => {
                        (local_copy.contains(name)
                            || matches!(name.as_str(), "Option" | "Result" | "PhantomData"))
                            && types.iter().all(|ty| ty.satisfied(local_copy))
                    }
                    Arguments::Unsupported => false,
                }
            }
        }
    }
}

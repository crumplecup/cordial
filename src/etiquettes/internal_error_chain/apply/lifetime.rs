//! A closed lifetime migration for one dynamic field and scalar context.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use syn::{Item, ItemFn, ItemImpl, ItemStruct, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::initializers::InitializerFacts;
use super::inventory::ErrorConversionUnit;
use super::{Edit, simple_type_name};

mod forwarding;
mod methods;
mod planner;

use planner::plan;

type Range = (usize, usize);

/// Located lifetime structure retained with the conversion unit in the IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LifetimePlanFacts {
    lifetime: String,
    type_name: String,
    field_type: Range,
    struct_ident_end: Option<usize>,
    struct_generics: Option<Range>,
    impls: Vec<ImplPlanFacts>,
    returns: Vec<ReturnPlanFacts>,
    initializers: InitializerFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ImplPlanFacts {
    self_type: Range,
    impl_token_end: Option<usize>,
    generics: Option<Range>,
    constructor_generics: Option<Range>,
    constructor_ident_end: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReturnPlanFacts {
    position: (usize, usize),
    range: Range,
}

impl LifetimePlanFacts {
    #[instrument(level = "trace", skip(self, source, strategy))]
    pub(super) fn edits(&self, source: &str, strategy: ErrorFieldStrategy) -> Option<Vec<Edit>> {
        let mut edits = self.initializers.edits(source, strategy)?;
        for site in &self.returns {
            edits.push(Edit {
                start: site.range.0,
                end: site.range.1,
                replacement: self.type_use(strategy),
            });
        }
        edits.push(Edit {
            start: self.field_type.0,
            end: self.field_type.1,
            replacement: match strategy {
                ErrorFieldStrategy::Borrowed => format!("&'{} str", self.lifetime),
                ErrorFieldStrategy::Owned => "String".to_string(),
            },
        });
        match strategy {
            ErrorFieldStrategy::Borrowed => {
                let end = self.struct_ident_end?;
                edits.push(Edit {
                    start: end,
                    end,
                    replacement: format!("<'{}>", self.lifetime),
                });
            }
            ErrorFieldStrategy::Owned => {
                let (start, end) = self.struct_generics?;
                edits.push(Edit {
                    start,
                    end,
                    replacement: String::new(),
                });
            }
        }
        for site in &self.impls {
            edits.push(Edit {
                start: site.self_type.0,
                end: site.self_type.1,
                replacement: self.type_use(strategy),
            });
            match strategy {
                ErrorFieldStrategy::Borrowed => {
                    let end = site.impl_token_end?;
                    edits.push(Edit {
                        start: end,
                        end,
                        replacement: format!("<'{}>", self.lifetime),
                    });
                    if let Some((start, end)) = site.constructor_generics {
                        edits.push(Edit {
                            start,
                            end,
                            replacement: String::new(),
                        });
                    }
                }
                ErrorFieldStrategy::Owned => {
                    let (start, end) = site.generics?;
                    edits.push(Edit {
                        start,
                        end,
                        replacement: String::new(),
                    });
                    if let Some(end) = site.constructor_ident_end {
                        edits.push(Edit {
                            start: end,
                            end,
                            replacement: format!("<'{}>", self.lifetime),
                        });
                    }
                }
            }
        }
        Some(edits)
    }

    #[instrument(level = "debug", skip(self, strategy))]
    fn type_use(&self, strategy: ErrorFieldStrategy) -> String {
        match strategy {
            ErrorFieldStrategy::Borrowed => format!("{}<'{}>", self.type_name, self.lifetime),
            ErrorFieldStrategy::Owned => self.type_name.clone(),
        }
    }
}

/// Top-level syntax facts shared by every lifetime candidate in one file.
pub(super) struct LifetimeFileIndex<'a> {
    structs: HashMap<String, &'a ItemStruct>,
    impls: HashMap<String, Vec<&'a ItemImpl>>,
    functions: Vec<&'a ItemFn>,
}

impl<'a> LifetimeFileIndex<'a> {
    #[instrument(level = "debug", skip(syntax))]
    pub(super) fn new(syntax: &'a syn::File) -> Self {
        let mut index = Self {
            structs: HashMap::new(),
            impls: HashMap::new(),
            functions: Vec::new(),
        };
        for item in &syntax.items {
            match item {
                Item::Struct(item) => {
                    index.structs.entry(item.ident.to_string()).or_insert(item);
                }
                Item::Impl(item) => {
                    if let Some(name) = simple_type_name(&item.self_ty) {
                        index.impls.entry(name).or_default().push(item);
                    }
                }
                Item::Fn(item) => index.functions.push(item),
                _ => {}
            }
        }
        index
    }
}

/// Collect a special lifetime migration from the shared parsed tree.
#[instrument(level = "debug", skip(file, index, unit, strategy, initializer_facts))]
pub(super) fn plan_for_unit(
    file: &SharedParsedFile,
    index: &LifetimeFileIndex<'_>,
    unit: &ErrorConversionUnit,
    strategy: ErrorFieldStrategy,
    initializer_facts: Option<InitializerFacts>,
) -> Option<LifetimePlanFacts> {
    let item = index.structs.get(unit.type_name())?;
    plan(
        file,
        index,
        item,
        unit.field_name(),
        unit,
        strategy,
        initializer_facts,
    )
}

#[instrument(level = "trace", skip(ty))]
fn scalar_copy_name(ty: &Type) -> Option<&syn::Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    let segment = path.path.segments.first()?;
    (matches!(
        segment.ident.to_string().as_str(),
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
    ) && matches!(segment.arguments, syn::PathArguments::None))
    .then_some(&segment.ident)
}

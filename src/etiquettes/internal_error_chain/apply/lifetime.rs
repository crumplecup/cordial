//! A closed lifetime migration for one dynamic field and scalar context.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::Edit;
use super::initializers::InitializerFacts;

mod forwarding;
mod methods;
mod native;
mod native_forwarding;
mod native_methods;
mod native_syntax;
mod planner;
mod source;

use source::{LifetimeFileIndex, scalar_copy_name};

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

/// Syntax evidence collected into the crate index before conversion units are
/// assembled. Closure and type-use checks are made against the IR unit later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LifetimeSiteFacts {
    field_name: String,
    strategy: ErrorFieldStrategy,
    plan: LifetimePlanFacts,
    allowed_type_uses: BTreeSet<(usize, usize)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct LifetimeFileFacts {
    sites: Vec<LifetimeSiteFacts>,
}

impl LifetimeFileFacts {
    #[instrument(level = "trace", skip(self, names))]
    pub(crate) fn select_names(&self, names: &BTreeSet<String>) -> Self {
        Self {
            sites: self
                .sites
                .iter()
                .filter(|site| names.contains(&site.plan.type_name))
                .cloned()
                .collect(),
        }
    }

    #[instrument(level = "debug", skip(self, strategy, sites, initializers))]
    pub(super) fn plan_for_unit(
        &self,
        type_name: &str,
        field_name: &str,
        strategy: ErrorFieldStrategy,
        sites: &[super::inventory::ErrorConversionSite],
        initializers: InitializerFacts,
    ) -> Option<LifetimePlanFacts> {
        use super::inventory::ErrorConversionSiteKind;
        let facts = self.sites.iter().find(|facts| {
            facts.plan.type_name == type_name
                && facts.field_name == field_name
                && facts.strategy == strategy
        })?;
        if sites.iter().any(|site| {
            site.kind() == ErrorConversionSiteKind::TypeUse
                && !facts
                    .allowed_type_uses
                    .contains(&(site.line(), site.column()))
        }) {
            return None;
        }
        let mut plan = facts.plan.clone();
        plan.initializers = initializers;
        Some(plan)
    }
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

//! Reduce collected expression sites into edits for one closed error graph.

use std::collections::{BTreeMap, BTreeSet};

use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::super::Edit;
use super::{Change, Shape, VariantChange};

mod facts;

pub(in crate::etiquettes::internal_error_chain::apply) use facts::GraphExprFacts;
use facts::{GraphExprEvent, GraphValueFact, SourceArgument};

type Range = (usize, usize);

pub(super) struct GraphExprPlanner<'a> {
    source: &'a str,
    facts: &'a GraphExprFacts,
    strategy: ErrorFieldStrategy,
    changes: &'a BTreeMap<(String, String), Change>,
    variants: &'a BTreeMap<(String, String), VariantChange>,
    edits: Vec<Edit>,
    valid: bool,
    constructed: BTreeSet<(String, String)>,
    constructed_variants: BTreeSet<(String, String)>,
    allowed_uses: BTreeSet<Range>,
    strict_external: bool,
    anchor_lifetime: Option<String>,
}

impl<'a> GraphExprPlanner<'a> {
    #[instrument(level = "debug", skip(source, facts, strategy, changes, variants))]
    pub(super) fn new(
        source: &'a str,
        facts: &'a GraphExprFacts,
        strategy: ErrorFieldStrategy,
        changes: &'a BTreeMap<(String, String), Change>,
        variants: &'a BTreeMap<(String, String), VariantChange>,
    ) -> Self {
        Self {
            source,
            facts,
            strategy,
            changes,
            variants,
            edits: Vec::new(),
            valid: true,
            constructed: BTreeSet::new(),
            constructed_variants: BTreeSet::new(),
            allowed_uses: BTreeSet::new(),
            strict_external: false,
            anchor_lifetime: None,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn strict_external(mut self) -> Self {
        self.strict_external = true;
        self
    }

    #[instrument(level = "trace", skip(self, lifetime))]
    pub(super) fn require_anchored(mut self, lifetime: &str) -> Self {
        self.anchor_lifetime = Some(lifetime.to_owned());
        self
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn finish(
        self,
    ) -> (
        Vec<Edit>,
        bool,
        BTreeSet<(String, String)>,
        BTreeSet<(String, String)>,
    ) {
        (
            self.edits,
            self.valid,
            self.constructed,
            self.constructed_variants,
        )
    }

    #[instrument(level = "debug", skip(self, range))]
    fn edit(&mut self, range: Option<Range>, replacement: String) {
        if let Some((start, end)) = range {
            self.edits.push(Edit {
                start,
                end,
                replacement,
            });
        } else {
            self.valid = false;
        }
    }

    #[instrument(level = "debug", skip(self, range))]
    fn allow(&mut self, range: Option<Range>) {
        if let Some(range) = range {
            self.allowed_uses.insert(range);
        } else {
            self.valid = false;
        }
    }

    #[instrument(level = "trace", skip(self, value))]
    fn anchored(&self, value: &GraphValueFact) -> bool {
        self.anchor_lifetime
            .as_ref()
            .is_none_or(|lifetime| value.anchored_lifetimes().contains(lifetime))
    }

    #[instrument(level = "debug", skip(self, value, shape))]
    fn expression(&self, value: &GraphValueFact, shape: Shape) -> Option<String> {
        let range = value.range()?;
        let text = self.source.get(range.0..range.1)?;
        match self.strategy {
            ErrorFieldStrategy::Borrowed => {
                let receiver = match shape {
                    Shape::String => value.borrowed_string(),
                    Shape::Value => value.borrowed_value(),
                    Shape::Boxed => value.borrowed_boxed(),
                }?;
                self.source.get(receiver.0..receiver.1).map(str::to_string)
            }
            ErrorFieldStrategy::Owned => Some(match shape {
                Shape::String => format!("({text}).to_owned()"),
                Shape::Value => format!("(*({text})).clone()"),
                Shape::Boxed => format!("Box::new((*({text})).clone())"),
            }),
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn run(&mut self) {
        for event in self.facts.events() {
            match event {
                GraphExprEvent::Struct {
                    name,
                    has_rest,
                    fields,
                } => {
                    if *has_rest && self.changes.keys().any(|key| key.0 == *name) {
                        self.valid = false;
                    }
                    for (field, value) in fields {
                        let key = (name.clone(), field.clone());
                        let Some(change) = self.changes.get(&key) else {
                            continue;
                        };
                        self.constructed.insert(key);
                        if field != "file" && !self.anchored(value) {
                            self.valid = false;
                        } else if let Some(replacement) = self.expression(value, change.shape) {
                            self.edit(value.range(), replacement);
                        } else {
                            self.valid = false;
                        }
                    }
                }
                GraphExprEvent::Variant {
                    owner,
                    variant,
                    args,
                } => {
                    let key = (owner.clone(), variant.clone());
                    let Some(change) = self.variants.get(&key) else {
                        continue;
                    };
                    self.constructed_variants.insert(key);
                    if let [value] = args.as_slice() {
                        if !self.anchored(value) {
                            self.valid = false;
                        } else if let Some(replacement) = self.expression(value, change.shape) {
                            self.edit(value.range(), replacement);
                        } else {
                            self.valid = false;
                        }
                    } else {
                        self.valid = false;
                    }
                }
                GraphExprEvent::SourceMethod { owner, argument } => {
                    if !self.changes.contains_key(&(owner.clone(), "source".into())) {
                        continue;
                    }
                    match (self.strategy, argument) {
                        (
                            ErrorFieldStrategy::Borrowed,
                            SourceArgument::BorrowedReference { field, reference },
                        ) => {
                            self.allow(*field);
                            self.edit(*reference, "self.source".into());
                        }
                        (ErrorFieldStrategy::Owned, SourceArgument::OwnedField { field }) => {
                            self.allow(*field);
                            self.edit(*field, "&self.source".into());
                        }
                        _ => self.valid = false,
                    }
                }
                GraphExprEvent::KindAsRef {
                    owner,
                    receiver,
                    whole,
                } => {
                    if self.strategy == ErrorFieldStrategy::Borrowed
                        && self.changes.contains_key(&(owner.clone(), "kind".into()))
                    {
                        self.allow(*receiver);
                        self.edit(*whole, "self.kind".into());
                    }
                }
                GraphExprEvent::KindMatch { owner, subject } => {
                    if self.strategy == ErrorFieldStrategy::Owned
                        && self.changes.contains_key(&(owner.clone(), "kind".into()))
                    {
                        self.allow(*subject);
                        self.edit(*subject, "self.kind.as_ref()".into());
                    }
                }
                GraphExprEvent::FieldUse {
                    owner,
                    field,
                    self_base,
                    range,
                } => {
                    if self.strict_external
                        && !self_base
                        && self
                            .changes
                            .keys()
                            .any(|(_, changed_field)| changed_field == field)
                    {
                        self.valid = false;
                    }
                    if *self_base
                        && self.changes.contains_key(&(owner.clone(), field.clone()))
                        && range.is_none_or(|range| !self.allowed_uses.contains(&range))
                    {
                        self.valid = false;
                    }
                }
                GraphExprEvent::Pattern { name, fields } => {
                    if fields
                        .iter()
                        .any(|field| self.changes.contains_key(&(name.clone(), field.clone())))
                    {
                        self.valid = false;
                    }
                }
                GraphExprEvent::Macro {
                    constructed_types,
                    unhandled_fields,
                    mentioned_types,
                } => {
                    if self.changes.keys().any(|(name, field)| {
                        constructed_types.contains(name) || unhandled_fields.contains(field)
                    }) || self
                        .variants
                        .keys()
                        .any(|(name, _)| mentioned_types.contains(name))
                    {
                        self.valid = false;
                    }
                }
            }
        }
    }
}

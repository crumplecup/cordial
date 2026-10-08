//! Lint resolution order: `After` is written, `Before` is the free inverse.

mod rank;

use std::borrow::Cow;

use tracing::instrument;

use crate::error::CordialResult;

pub use crate::etiquette::order_table::{BUILT_IN_ORDER, DERIVE_RULE_IDS};
#[cfg(feature = "quality")]
pub(crate) use rank::sort_quality_etiquettes;
pub(crate) use rank::table_is_valid;

/// Written side of the pair: this lint runs after the returned ids.
pub trait After {
    /// Predecessor lint ids (`Rule::id` strings).
    fn after(&self) -> &[&str];
    /// Why this lint follows those predecessors.
    fn explain(&self) -> OrderExplain;
}

/// Free inverse of [`After`]: this lint runs before the returned ids.
pub trait Before {
    /// Successor lint ids.
    fn before(&self) -> &[Cow<'static, str>];
    /// Rationale copied from the written `After`.
    fn explain(&self) -> OrderExplain;
}

/// Why this lint runs after its predecessors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderExplain {
    title: &'static str,
    body: &'static str,
}

impl OrderExplain {
    /// Bind a static order rationale.
    pub const fn new(title: &'static str, body: &'static str) -> Self {
        Self { title, body }
    }

    /// One-line title for `cordial explain`.
    pub const fn title(&self) -> &'static str {
        self.title
    }

    /// Human-readable rationale.
    pub const fn body(&self) -> &'static str {
        self.body
    }
}

/// Written `After` declaration: this lint runs after `after`.
///
/// Like `From`, this is the impl you write. The inverse `Before` is derived.
/// Construct only in `const` / `static` items so the row stays a table, not a
/// runtime ADT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintConstraint {
    /// Lint id that runs after [`Self::after`].
    id: &'static str,
    /// Predecessor lint ids.
    after: &'static [&'static str],
    /// Why this constraint exists.
    explain: OrderExplain,
}

impl LintConstraint {
    /// Bind one `After` row.
    pub const fn new(
        id: &'static str,
        after: &'static [&'static str],
        explain: OrderExplain,
    ) -> Self {
        Self { id, after, explain }
    }

    /// Lint id that runs after [`Self::after`].
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// Predecessor lint ids.
    pub const fn after(&self) -> &'static [&'static str] {
        self.after
    }

    /// Why this constraint exists.
    pub const fn explain(&self) -> OrderExplain {
        self.explain
    }
}

impl After for LintConstraint {
    #[instrument(level = "trace", skip(self))]
    fn after(&self) -> &[&str] {
        self.after
    }

    #[instrument(level = "trace", skip(self))]
    fn explain(&self) -> OrderExplain {
        self.explain
    }
}

/// Derived `Before` view for one predecessor id.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct BeforeMirror {
    before: Vec<Cow<'static, str>>,
    #[getter(skip)]
    explain: OrderExplain,
}

impl BeforeMirror {
    /// Rationale from the `After` that created the edge.
    pub const fn explain(&self) -> OrderExplain {
        self.explain
    }
}

impl Before for BeforeMirror {
    #[instrument(level = "trace", skip(self))]
    fn before(&self) -> &[Cow<'static, str>] {
        &self.before
    }

    #[instrument(level = "trace", skip(self))]
    fn explain(&self) -> OrderExplain {
        self.explain
    }
}

/// Validated lint-order DAG. Built-ins live in [`BUILT_IN_ORDER`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintOrder {
    known_ids: Vec<Cow<'static, str>>,
    constraints: Vec<LintConstraint>,
}

impl LintOrder {
    #[instrument(level = "debug")]
    pub(crate) fn empty() -> Self {
        Self {
            known_ids: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Validate `constraints` against `known_ids` and keep owned rows.
    #[instrument(level = "debug", skip(constraints))]
    pub fn try_from(
        known_ids: &'static [&'static str],
        constraints: &[LintConstraint],
    ) -> CordialResult<Self> {
        rank::validate(known_ids, constraints)?;
        Ok(Self {
            known_ids: known_ids.iter().map(|id| Cow::Borrowed(*id)).collect(),
            constraints: constraints.to_vec(),
        })
    }

    /// Written `After` row for this lint, if any.
    #[instrument(level = "trace", skip(self))]
    pub fn constraint(&self, id: &str) -> Option<&LintConstraint> {
        self.constraints.iter().find(|row| row.id() == id)
    }

    /// Ids this lint runs after.
    #[instrument(level = "trace", skip(self))]
    pub fn predecessors(&self, id: &str) -> Vec<&str> {
        self.constraint(id)
            .map(|row| row.after().to_vec())
            .unwrap_or_default()
    }

    /// Ids this lint runs before (free inverse of `After`).
    #[instrument(level = "trace", skip(self))]
    pub fn successors(&self, id: &str) -> Vec<&'static str> {
        self.constraints
            .iter()
            .filter(|row| row.after().contains(&id))
            .map(|row| row.id)
            .collect()
    }

    /// `Before` view for a predecessor id, when any `After` names it.
    #[instrument(level = "debug", skip(self))]
    pub fn before_mirror(&self, id: &str) -> Option<BeforeMirror> {
        let explain = self
            .constraints
            .iter()
            .find(|row| row.after().contains(&id))?
            .explain();
        let before: Vec<Cow<'static, str>> =
            self.successors(id).into_iter().map(Cow::Borrowed).collect();
        if before.is_empty() {
            return None;
        }
        Some(BeforeMirror { before, explain })
    }

    /// Unique written `After` rows that apply to any id in `ids`.
    #[instrument(level = "debug", skip(self, ids))]
    pub fn constraints_for(&self, ids: &[&str]) -> Vec<&LintConstraint> {
        let mut rows = Vec::new();
        for row in &self.constraints {
            if ids.contains(&row.id())
                && !rows
                    .iter()
                    .any(|seen: &&LintConstraint| seen.id() == row.id())
            {
                rows.push(row);
            }
        }
        rows
    }

    /// Known ids in Kahn order, then lexicographic within a wave.
    #[instrument(level = "trace", skip(self))]
    pub fn ranked_ids(&self) -> Vec<&'static str> {
        rank::ranked_ids(&self.known_ids, &self.constraints)
    }
}

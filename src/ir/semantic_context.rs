//! Cfg contexts: the same files analysed under different cfg sets in one database.
//!
//! rust-analyzer resolves only code active under a crate's cfg set. Code gated
//! by a verifier cfg (`cfg(kani)`, `cfg(creusot)`, ...) is inactive in the
//! ordinary build and has no semantic facts there. The loader analyses the same
//! files again with each such cfg set enabled; every analysis is a context, and
//! each fact says which context resolved it.

use serde::{Deserialize, Serialize};

use tracing::instrument;
/// Node attribute holding a function's facts in the non-ordinary contexts that
/// resolved it; the ordinary context's facts stay in
/// [`ATTR_SEMANTIC_FUNCTION`](super::ATTR_SEMANTIC_FUNCTION).
pub const ATTR_SEMANTIC_CONTEXT_FUNCTIONS: &str = "semantic_context_functions";

/// Identity of one cfg context: the cfg atoms enabled and disabled on top of the
/// ordinary build. The ordinary context changes none.
///
/// Atoms use cfg's own spelling: a flag is `kani`, a key-value pair is
/// `feature = "alternate"` written as `feature="alternate"`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Default,
    derive_getters::Getters,
)]
pub struct SemanticContextKey {
    /// Atoms this context turns on, sorted.
    enabled: Vec<String>,
    /// Atoms this context turns off, sorted.
    disabled: Vec<String>,
}

impl SemanticContextKey {
    /// The ordinary build, which changes no cfg atom.
    #[instrument(level = "debug")]
    #[hotpath::measure]
    pub fn ordinary() -> Self {
        Self::default()
    }

    /// A context enabling and disabling the given atoms; order and repeats do
    /// not matter. An atom named in both is disabled-and-enabled at once, which
    /// no build can satisfy, so it is kept in neither list.
    #[instrument(level = "debug", skip(enabled, disabled))]
    #[hotpath::measure]
    pub fn new(
        enabled: impl IntoIterator<Item = impl Into<String>>,
        disabled: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let sorted = |atoms: &mut Vec<String>| {
            atoms.sort();
            atoms.dedup();
        };
        let mut enabled: Vec<String> = enabled.into_iter().map(Into::into).collect();
        let mut disabled: Vec<String> = disabled.into_iter().map(Into::into).collect();
        sorted(&mut enabled);
        sorted(&mut disabled);
        let contradictory: Vec<String> = enabled
            .iter()
            .filter(|atom| disabled.contains(atom))
            .cloned()
            .collect();
        enabled.retain(|atom| !contradictory.contains(atom));
        disabled.retain(|atom| !contradictory.contains(atom));
        Self { enabled, disabled }
    }

    /// A context enabling `atoms` and disabling nothing.
    #[instrument(level = "debug", skip(atoms))]
    #[hotpath::measure]
    pub fn enabling(atoms: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::new(atoms, std::iter::empty::<String>())
    }

    /// Whether this is the ordinary build.
    #[instrument(level = "trace", skip(self))]
    pub fn is_ordinary(&self) -> bool {
        self.enabled.is_empty() && self.disabled.is_empty()
    }
}

impl std::fmt::Display for SemanticContextKey {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_ordinary() {
            return formatter.write_str("ordinary");
        }
        let atoms: Vec<String> = self
            .enabled
            .iter()
            .map(|atom| format!("+{atom}"))
            .chain(self.disabled.iter().map(|atom| format!("-{atom}")))
            .collect();
        formatter.write_str(&atoms.join(" "))
    }
}

/// One function's facts as resolved in one non-ordinary context.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticContextFunction {
    context: SemanticContextKey,
    function: super::SemanticFunction,
}

impl SemanticContextFunction {
    /// These facts with every identity in them replaced by `map`'s answer.
    #[tracing::instrument(level = "trace", skip(self, map))]
    pub(crate) fn map_identities(
        mut self,
        map: &mut impl FnMut(super::SemanticItemId) -> super::SemanticItemId,
    ) -> Self {
        self.function = self.function.map_identities(map);
        self
    }
}

/// Node attribute listing the contexts in which a function is active, as
/// indexes into [`SemanticContextPlan::contexts`]. Cheap enough to record for
/// every function, unlike the facts themselves.
pub const ATTR_SEMANTIC_ACTIVITY: &str = "semantic_activity";

/// Crate attribute recording which non-ordinary contexts were analysed.
pub const ATTR_SEMANTIC_CONTEXT_PLAN: &str = "semantic_context_plan";

/// The non-ordinary contexts analysed for one crate projection.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticContextPlan {
    /// Analysed contexts; a function's activity indexes into this list.
    contexts: Vec<SemanticContextKey>,
    /// Whether planning reached a fixed point. When false, gated code beyond the
    /// analysed contexts reports no active context: a gap, not a proof.
    complete: bool,
}

/// Which analysed cfg contexts leave each function of one crate active, read from
/// the crate's evidence once. The snapshot and the plan are whole-crate records;
/// decoding them per function made a pass over every function cost the size of
/// the project for each of them.
#[derive(Debug, Clone)]
pub struct ContextActivity {
    plan: Option<SemanticContextPlan>,
}

impl ContextActivity {
    /// Wrap a crate's decoded plan, if it has one.
    #[tracing::instrument(level = "trace", skip(plan))]
    pub fn new(plan: Option<SemanticContextPlan>) -> Self {
        Self { plan }
    }

    /// The contexts in which `id` is active: the ordinary one when the source
    /// database resolved it, then each planned context its activity names.
    #[tracing::instrument(level = "trace", skip(self, view), err(level = "warn"))]
    pub fn active(
        &self,
        view: &(impl crate::ir::IrView + ?Sized),
        id: crate::NodeId,
    ) -> crate::CordialResult<Vec<SemanticContextKey>> {
        let mut active = Vec::new();
        if view
            .node(id)
            .is_some_and(|node| node.has_attr(super::ATTR_SEMANTIC_FUNCTION))
        {
            active.push(SemanticContextKey::ordinary());
        }
        if let (Some(plan), Some(indexes)) = (
            &self.plan,
            view.node(id)
                .and_then(|node| node.attr(ATTR_SEMANTIC_ACTIVITY))
                .map(<Vec<u32> as serde::Deserialize>::deserialize)
                .transpose()?,
        ) {
            active.extend(
                indexes
                    .into_iter()
                    .filter_map(|index| plan.contexts().get(index as usize).cloned()),
            );
        }
        Ok(active)
    }
}

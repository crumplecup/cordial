//! Reduce shared lifetime sites into one strategy-specific edit plan.

use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::super::super::Edit;
use super::{GraphLifetimeFacts, GraphLifetimePlan, Range, TypeArguments};

impl GraphLifetimeFacts {
    #[instrument(level = "debug", skip(self, plan, strategy, storage_edits))]
    pub(in crate::etiquettes::internal_error_chain::apply::graph_apply::lifetimes) fn plan(
        &self,
        plan: &GraphLifetimePlan,
        strategy: ErrorFieldStrategy,
        storage_edits: &[Edit],
        allow_qualified: bool,
    ) -> Option<Vec<Edit>> {
        let mut edits = Vec::new();
        let mut valid = true;
        let lifetime = plan.lifetime();
        for site in &self.declarations {
            if !plan.names().contains(&site.name) {
                continue;
            }
            if site.module_depth > 0 {
                valid = false;
                continue;
            }
            match strategy {
                ErrorFieldStrategy::Borrowed if site.generics_empty => push_insert(
                    &mut edits,
                    &mut valid,
                    site.ident_end,
                    format!("<'{lifetime}>"),
                ),
                ErrorFieldStrategy::Owned
                    if site.one_lifetime.as_deref() == Some(lifetime.as_str()) =>
                {
                    push_edit(&mut edits, &mut valid, site.generics_range, String::new());
                }
                _ => valid = false,
            }
        }
        for site in &self.impls {
            let Some(name) = &site.name else { continue };
            if !plan.names().contains(name) {
                continue;
            }
            if site.module_depth > 0 {
                valid = false;
                continue;
            }
            let matching_lifetime = site.one_lifetime.as_deref() == Some(lifetime.as_str());
            let mut moves_lifetime = false;
            match strategy {
                ErrorFieldStrategy::Borrowed if site.generics_empty => push_insert(
                    &mut edits,
                    &mut valid,
                    site.impl_token_end,
                    format!("<'{lifetime}>"),
                ),
                ErrorFieldStrategy::Borrowed if matching_lifetime => {}
                ErrorFieldStrategy::Owned if matching_lifetime => {
                    if !site.trait_lifetimes.contains(lifetime) {
                        push_edit(&mut edits, &mut valid, site.generics_range, String::new());
                        moves_lifetime = true;
                    }
                }
                _ => valid = false,
            }
            for method in &site.methods {
                if strategy == ErrorFieldStrategy::Borrowed
                    && method.output_self
                    && !method.safe_constructor_lifetimes.contains(lifetime)
                {
                    valid = false;
                }
                match strategy {
                    ErrorFieldStrategy::Borrowed if method.generics_empty => {}
                    ErrorFieldStrategy::Borrowed
                        if method.one_lifetime.as_deref() == Some(lifetime.as_str())
                            && method.output_self
                            && method.all_inputs_typed =>
                    {
                        push_edit(&mut edits, &mut valid, method.generics_range, String::new());
                    }
                    ErrorFieldStrategy::Borrowed => valid = false,
                    ErrorFieldStrategy::Owned
                        if moves_lifetime && method.used_lifetimes.contains(lifetime) =>
                    {
                        if method.generics_empty {
                            push_insert(
                                &mut edits,
                                &mut valid,
                                method.ident_end,
                                format!("<'{lifetime}>"),
                            );
                        } else {
                            valid = false;
                        }
                    }
                    ErrorFieldStrategy::Owned => {}
                }
            }
        }
        if strategy == ErrorFieldStrategy::Borrowed {
            for site in &self.functions {
                if !site.return_names.is_disjoint(plan.names())
                    && site
                        .safe_returns
                        .get(lifetime)
                        .is_none_or(|safe| safe.is_disjoint(plan.names()))
                {
                    valid = false;
                }
            }
        }
        for site in &self.type_paths {
            if !plan.names().contains(&site.name)
                || site.range.is_some_and(|(start, end)| {
                    storage_edits
                        .iter()
                        .any(|edit| edit.start <= start && end <= edit.end && edit.start < edit.end)
                })
            {
                continue;
            }
            let scope = site.scope_lifetimes.contains(lifetime)
                || site.conditional_scope.as_ref().is_some_and(|conditional| {
                    plan.names().contains(&conditional.owner)
                        && (!conditional.borrowed_only || strategy == ErrorFieldStrategy::Borrowed)
                });
            if !scope || site.qself || (!allow_qualified && site.path_len != 1) {
                valid = false;
                continue;
            }
            match (&strategy, &site.arguments) {
                (ErrorFieldStrategy::Borrowed, TypeArguments::None) => push_insert(
                    &mut edits,
                    &mut valid,
                    site.ident_end,
                    format!("<'{lifetime}>"),
                ),
                (ErrorFieldStrategy::Owned, TypeArguments::OneLifetime { name, range })
                    if name == lifetime =>
                {
                    push_edit(&mut edits, &mut valid, *range, String::new());
                }
                _ => valid = false,
            }
        }
        if !self.expr_path_blockers.is_disjoint(plan.names()) {
            valid = false;
        }
        valid.then_some(edits)
    }
}

#[instrument(level = "trace", skip(edits, valid))]
fn push_insert(edits: &mut Vec<Edit>, valid: &mut bool, end: Option<usize>, replacement: String) {
    push_edit(edits, valid, end.map(|end| (end, end)), replacement);
}

#[instrument(level = "trace", skip(edits, valid, range))]
fn push_edit(edits: &mut Vec<Edit>, valid: &mut bool, range: Option<Range>, replacement: String) {
    if let Some((start, end)) = range {
        edits.push(Edit {
            start,
            end,
            replacement,
        });
    } else {
        *valid = false;
    }
}

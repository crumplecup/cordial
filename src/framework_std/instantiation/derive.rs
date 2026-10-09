//! Default instantiations, derived from rustdoc alone.
//!
//! A generic type's declared bounds say which types are valid arguments, and
//! rustdoc lists which types implement each bounding trait. The candidates
//! for a parameter are the types that implement all of its bounds; the
//! instantiations are the combinations of one candidate per parameter. Nothing
//! is asked of the library's author or of the coverage source.

use tracing::instrument;

use crate::framework_std::type_identity::{DeclaredBound, TypeKey, TypeResolver};

use super::note::describe_bounds;

/// What the derivation found for one generic type.
pub(super) enum Derived {
    /// Candidate argument tuples, and what they were derived from.
    Tuples {
        tuples: Vec<Vec<TypeKey>>,
        basis: String,
    },
    /// Nothing could be derived, and why.
    Unavailable(String),
}

/// Derive the instantiations of the generic type `head`, at most `cap` of them.
#[instrument(level = "debug", skip(head, resolver))]
pub(super) fn derive(head: &TypeKey, resolver: &dyn TypeResolver, cap: usize) -> Derived {
    let params = resolver.parameters(head);
    if params.is_empty() {
        return Derived::Unavailable(
            "the type is not generic, or its crate is not readable".into(),
        );
    }
    let bounds = resolver.declared_bounds(head);
    if bounds.is_empty() {
        return Derived::Unavailable("the type declares no bounds on its parameters".into());
    }
    let mut per_param: Vec<Vec<TypeKey>> = Vec::with_capacity(params.len());
    for (index, name) in params.iter().enumerate() {
        let on_param: Vec<&DeclaredBound> = bounds
            .iter()
            .filter(|declared| declared.index() == index)
            .collect();
        if on_param.is_empty() {
            return Derived::Unavailable(format!("parameter `{name}` declares no bounds"));
        }
        match candidates(&on_param, resolver) {
            Candidates::Found(found) if !found.is_empty() => per_param.push(found),
            Candidates::Found(_) => {
                return Derived::Unavailable(format!(
                    "no type implements the bounds on `{name}` in the readable crates"
                ));
            }
            Candidates::Unidentified(reason) => return Derived::Unavailable(reason),
        }
    }
    let total = per_param
        .iter()
        .try_fold(1usize, |acc, found| acc.checked_mul(found.len()));
    match total {
        Some(count) if count <= cap => Derived::Tuples {
            tuples: product(&per_param),
            basis: format!("the implementors of {}", describe_bounds(&bounds)),
        },
        _ => Derived::Unavailable(format!(
            "the combinations exceed the cap of {cap}; list the instantiations to track"
        )),
    }
}

/// The types implementing every bound on one parameter.
enum Candidates {
    /// The types that implement all of the parameter's bounds; possibly none.
    Found(Vec<TypeKey>),
    /// A bound could not be identified, so the parameter cannot be checked.
    Unidentified(String),
}

#[instrument(level = "trace", skip(on_param, resolver))]
fn candidates(on_param: &[&DeclaredBound], resolver: &dyn TypeResolver) -> Candidates {
    let mut found: Option<Vec<TypeKey>> = None;
    for declared in on_param {
        let bound = match declared.bound() {
            Ok(bound) => bound,
            Err(reason) => {
                return Candidates::Unidentified(format!(
                    "the bound on `{}` could not be identified: {reason}",
                    declared.param()
                ));
            }
        };
        let implementors = resolver.implementors(bound);
        found = Some(match found {
            None => implementors,
            Some(so_far) => so_far
                .into_iter()
                .filter(|key| implementors.contains(key))
                .collect(),
        });
    }
    Candidates::Found(found.unwrap_or_default())
}

/// Every way of picking one candidate per parameter, in a stable order.
#[instrument(level = "trace", skip(per_param))]
fn product(per_param: &[Vec<TypeKey>]) -> Vec<Vec<TypeKey>> {
    per_param.iter().fold(vec![Vec::new()], |acc, options| {
        acc.iter()
            .flat_map(|prefix| {
                options.iter().map(move |option| {
                    let mut next = prefix.clone();
                    next.push(option.clone());
                    next
                })
            })
            .collect()
    })
}

//! The human-readable notes on a parent row.

use tracing::instrument;

use crate::framework_std::AmenableStdEntry;
use crate::framework_std::AmenableStdStatus;
use crate::framework_std::type_identity::{DeclaredBound, Implements, TypeKey, TypeResolver};

/// One instantiation, as the bounds note sees it.
#[derive(derive_new::new, derive_getters::Getters)]
pub(super) struct ChildView<'a> {
    /// Short label for the arguments (`Utc`, `FixedOffset`).
    short: String,
    /// The resolved key, when the instantiation could be identified.
    #[getter(copy)]
    key: Option<&'a TypeKey>,
}

/// "3 of 4 instantiations Complete", counting only those not excepted.
#[instrument(level = "debug", skip(children))]
pub(super) fn rollup_summary(children: &[AmenableStdEntry]) -> String {
    let counted: Vec<&AmenableStdEntry> = children
        .iter()
        .filter(|child| child.status() != AmenableStdStatus::Skipped)
        .collect();
    let complete = counted
        .iter()
        .filter(|child| child.status() == AmenableStdStatus::Complete)
        .count();
    let excepted = children.len() - counted.len();
    let mut text = format!("{complete} of {} instantiations Complete", counted.len());
    if excepted > 0 {
        text.push_str(&format!(" ({excepted} excepted)"));
    }
    text
}

/// A short label for a key's type arguments: `Utc`, or `Tz` for
/// `chrono_tz::Tz`.
#[instrument(level = "trace", skip(key))]
pub(super) fn short_label(key: &TypeKey) -> String {
    key.args()
        .iter()
        .map(|arg| {
            arg.head()
                .rsplit("::")
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the generic type itself requires of its parameters, and which
/// instantiations meet it.
///
/// The bounds are the ones the type declares in its own definition, read from
/// rustdoc (`struct DateTime<Tz: TimeZone>`): per parameter, matched to each
/// instantiation's argument by position, with nothing supplied by the
/// registry. A type that declares none (`HashMap` keeps its bounds on impl
/// blocks) says so rather than guessing.
#[instrument(level = "debug", skip(head, children, resolver))]
pub(super) fn declared_bounds_note(
    head: &TypeKey,
    children: &[ChildView<'_>],
    resolver: &dyn TypeResolver,
) -> String {
    let bounds = resolver.declared_bounds(head);
    if bounds.is_empty() {
        return "the type declares no bounds on its parameters".to_string();
    }
    let mut text = format!("declared bounds {}", describe_bounds(&bounds));
    let mut satisfied = Vec::new();
    let mut missing = Vec::new();
    let mut unknown = Vec::new();
    for child in children {
        let Some(key) = child.key() else {
            unknown.push(format!("{} (unidentified)", child.short()));
            continue;
        };
        match check(&bounds, key, resolver) {
            Outcome::Meets => satisfied.push(child.short().clone()),
            Outcome::Lacks(which) => missing.push(format!("{} ({which})", child.short())),
            Outcome::Unknown(reason) => unknown.push(format!("{} ({reason})", child.short())),
        }
    }
    if !satisfied.is_empty() {
        text.push_str(&format!(": satisfied by {}", satisfied.join(", ")));
    }
    if !missing.is_empty() {
        text.push_str(&format!(
            "; no direct impl found for {}",
            missing.join(", ")
        ));
    }
    if !unknown.is_empty() {
        text.push_str(&format!("; unknown for {}", unknown.join(", ")));
    }
    text
}

/// `Tz: TimeZone` or `K: Hash + Eq, V: Clone`, each in backticks.
#[instrument(level = "trace", skip(bounds))]
pub(super) fn describe_bounds(bounds: &[DeclaredBound]) -> String {
    let mut params: Vec<(&str, Vec<String>)> = Vec::new();
    for bound in bounds {
        let name = match bound.bound() {
            Ok(key) => key
                .head()
                .rsplit("::")
                .next()
                .unwrap_or_default()
                .to_string(),
            Err(_) => "?".to_string(),
        };
        match params.iter_mut().find(|(param, _)| *param == bound.param()) {
            Some((_, names)) => names.push(name),
            None => params.push((bound.param(), vec![name])),
        }
    }
    params
        .iter()
        .map(|(param, names)| format!("`{param}: {}`", names.join(" + ")))
        .collect::<Vec<_>>()
        .join(", ")
}

/// How one instantiation fares against the declared bounds.
enum Outcome {
    /// Every declared bound has a direct impl on its argument.
    Meets,
    /// The first bound with no direct impl found, as `Param: Trait`.
    Lacks(String),
    /// A bound or argument could not be checked, with the reason.
    Unknown(String),
}

/// Check each declared bound against the argument at its parameter's
/// position. A missing impl outranks an unchecked bound, so the answer names
/// a definite failure when there is one.
#[instrument(level = "debug", skip(bounds, key, resolver))]
fn check(bounds: &[DeclaredBound], key: &TypeKey, resolver: &dyn TypeResolver) -> Outcome {
    let mut lacks = None;
    let mut unknown = None;
    for declared in bounds {
        let label = |bound: &TypeKey| {
            format!(
                "{}: {}",
                declared.param(),
                bound.head().rsplit("::").next().unwrap_or_default()
            )
        };
        let Ok(bound) = declared.bound() else {
            unknown.get_or_insert(format!("bound on `{}` not identified", declared.param()));
            continue;
        };
        let Some(arg) = key.args().get(declared.index()) else {
            unknown.get_or_insert(format!("no argument for `{}`", declared.param()));
            continue;
        };
        match resolver.implements(arg, bound) {
            Implements::Yes => {}
            Implements::NoDirectImpl => {
                lacks.get_or_insert(label(bound));
            }
            Implements::Unknown(reason) => {
                unknown.get_or_insert(reason);
            }
        }
    }
    match (lacks, unknown) {
        (Some(which), _) => Outcome::Lacks(which),
        (None, Some(reason)) => Outcome::Unknown(reason),
        (None, None) => Outcome::Meets,
    }
}

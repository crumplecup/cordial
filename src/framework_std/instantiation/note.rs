//! The human-readable notes on a parent row.

use tracing::instrument;

use crate::framework_std::AmenableStdEntry;
use crate::framework_std::AmenableStdStatus;
use crate::framework_std::type_identity::{EvidenceKey, Implements, TypeKey, TypeResolver};

/// One instantiation, as the generic-claim note sees it.
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
#[instrument(level = "debug", skip(key))]
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

/// How the generic claims on a type relate to its instantiations: the bounds
/// and premises each relies on, and which instantiations satisfy the bounds.
#[instrument(level = "debug", skip(claims, children, resolver))]
pub(super) fn claims_note(
    claims: &[&EvidenceKey],
    children: &[ChildView<'_>],
    resolver: &dyn TypeResolver,
) -> Option<String> {
    if claims.is_empty() {
        return None;
    }
    let parts: Vec<String> = claims
        .iter()
        .map(|claim| claim_text(claim, children, resolver))
        .collect();
    Some(parts.join("; "))
}

#[instrument(level = "debug", skip(claim, children, resolver))]
fn claim_text(
    claim: &EvidenceKey,
    children: &[ChildView<'_>],
    resolver: &dyn TypeResolver,
) -> String {
    let mut text = format!("generic claim `{}`", claim.name());
    if !claim.bounds().is_empty() {
        text.push_str(&format!(" over `{}`", claim.bounds().join(" + ")));
    }
    if !claim.premises().is_empty() {
        text.push_str(&format!(" (premises: {})", claim.premises().join(", ")));
    }
    if claim.bounds().is_empty() || children.is_empty() {
        return text;
    }
    let mut satisfied = Vec::new();
    let mut not_found = Vec::new();
    let mut unknown = Vec::new();
    for child in children {
        let Some(key) = child.key() else {
            unknown.push(format!("{} (unidentified)", child.short()));
            continue;
        };
        match combined_over_args(claim.bounds(), key, resolver) {
            Implements::Yes => satisfied.push(child.short().clone()),
            Implements::NoDirectImpl => not_found.push(child.short().clone()),
            Implements::Unknown(reason) => unknown.push(format!("{} ({reason})", child.short())),
        }
    }
    if !satisfied.is_empty() {
        text.push_str(&format!(": satisfied by {}", satisfied.join(", ")));
    }
    if !not_found.is_empty() {
        text.push_str(&format!(
            "; no direct impl found for {}",
            not_found.join(", ")
        ));
    }
    if !unknown.is_empty() {
        text.push_str(&format!("; unknown for {}", unknown.join(", ")));
    }
    text
}

/// A generic claim's bounds constrain the type *arguments* (`Tz: TimeZone`
/// on `DateTime<Tz>`), not the instantiation. The bounds carry no parameter
/// names, so each is applied to every argument: right for the single-parameter
/// generics the targets have today, and conservative (never a false `Yes`)
/// for others.
#[instrument(level = "debug", skip(key, resolver))]
fn combined_over_args(bounds: &[String], key: &TypeKey, resolver: &dyn TypeResolver) -> Implements {
    if key.args().is_empty() {
        return Implements::Unknown("no type arguments".to_string());
    }
    let mut worst = Implements::Yes;
    for arg in key.args() {
        match combined(bounds, arg, resolver) {
            Implements::Yes => {}
            unknown @ Implements::Unknown(_) => return unknown,
            Implements::NoDirectImpl => worst = Implements::NoDirectImpl,
        }
    }
    worst
}

/// All bounds must hold: `Yes` only if every one does, `Unknown` if any
/// could not be checked, else the first missing impl.
#[instrument(level = "debug", skip(key, resolver))]
fn combined(bounds: &[String], key: &TypeKey, resolver: &dyn TypeResolver) -> Implements {
    let mut worst = Implements::Yes;
    for bound in bounds {
        match resolver.implements(key, bound) {
            Implements::Yes => {}
            unknown @ Implements::Unknown(_) => return unknown,
            Implements::NoDirectImpl => worst = Implements::NoDirectImpl,
        }
    }
    worst
}

//! Visibility, signature, and receiver predicates.

use syn::{FnArg, Receiver, ReceiverKind, ReturnType, Signature, Visibility};
use tracing::instrument;

#[instrument(level = "debug", skip(vis))]
pub(in crate::etiquettes::derives) fn field_is_exposed(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

#[instrument(level = "debug", skip(sig))]
pub(in crate::etiquettes::derives) fn constructor_arg_count(sig: &Signature) -> usize {
    sig.inputs
        .iter()
        .filter(|arg| !matches!(arg, FnArg::Receiver(_)))
        .count()
}

#[instrument(level = "debug", skip(sig))]
pub(in crate::etiquettes::derives) fn consumes_self(sig: &Signature) -> bool {
    matches!(sig.receiver(), Some(recv) if receiver_is_value_or_typed(recv))
}

#[instrument(level = "trace", skip(sig))]
pub(in crate::etiquettes::derives) fn is_fluent_setter(sig: &Signature) -> bool {
    if !matches!(sig.output, ReturnType::Type(_, _)) {
        return false;
    }
    let Some(recv) = sig.receiver() else {
        return false;
    };
    receiver_is_mut_value(recv) && sig.inputs.len() >= 2
}

#[instrument(level = "trace", skip(recv), ret)]
pub(in crate::etiquettes::derives) fn receiver_is_immutable_reference(recv: &Receiver) -> bool {
    matches!(recv.kind, ReceiverKind::Reference(_, _, None)) && recv.mutability.is_none()
}

#[instrument(level = "trace", skip(recv), ret)]
pub(in crate::etiquettes::derives) fn receiver_is_mutable(recv: &Receiver) -> bool {
    recv.mutability.is_some() || matches!(recv.kind, ReceiverKind::Reference(_, _, Some(_)))
}

#[instrument(level = "trace", skip(recv), ret)]
fn receiver_is_value_or_typed(recv: &Receiver) -> bool {
    matches!(recv.kind, ReceiverKind::Value | ReceiverKind::Typed(_, _))
}

#[instrument(level = "trace", skip(recv), ret)]
fn receiver_is_mut_value(recv: &Receiver) -> bool {
    matches!(recv.kind, ReceiverKind::Value) && recv.mutability.is_some()
}

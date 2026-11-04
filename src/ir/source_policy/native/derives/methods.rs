//! Method signature and body facts.
use super::{constructor, exprs, setters};
use crate::{
    CordialResult,
    ir::{DeriveMethodFacts, DeriveMethodFlags},
    loader::DatabaseSource,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs},
};

#[tracing::instrument(level = "trace", skip(method, file), err(level = "warn"))]
pub(super) fn collect(
    method: &ast::Fn,
    ty: &str,
    file: &DatabaseSource<'_>,
) -> CordialResult<DeriveMethodFacts> {
    let name = super::name(method)?;
    let receiver = method.param_list().and_then(|list| list.self_param());
    let typed = receiver
        .as_ref()
        .is_some_and(|recv| recv.colon_token().is_some());
    let reference = receiver
        .as_ref()
        .is_some_and(|recv| recv.amp_token().is_some());
    let mutable = receiver
        .as_ref()
        .is_some_and(|recv| recv.mut_token().is_some());
    let body = method
        .body()
        .ok_or_else(|| crate::CordialError::not_found("derive method body".into()))?;
    let statements = exprs::statements(&body);
    let target = name
        .strip_prefix("with_")
        .or_else(|| name.strip_prefix("set_"))
        .unwrap_or(&name);
    let setter = setters::classify(&statements, target, &exprs::params(method));
    let field_read = match statements.as_slice() {
        [Some(expr)] => exprs::field_read(expr.clone()),
        _ => None,
    };
    let return_reference = match method.ret_type().and_then(|ret| ret.ty()) {
        Some(ast::Type::RefType(ty)) => ty.ty().map(super::super::context::type_label),
        _ => None,
    };
    let flags = DeriveMethodFlags::new(
        method.const_token().is_some(),
        method
            .attrs()
            .any(|attr| attr.simple_name().as_deref() == Some("track_caller")),
        !typed && reference && !mutable,
        mutable,
        receiver.is_some() && (!reference || typed),
        !typed
            && !reference
            && mutable
            && method.ret_type().is_some()
            && method
                .param_list()
                .is_some_and(|list| list.params().next().is_some()),
    );
    Ok(DeriveMethodFacts::new(
        name,
        file.line(method.syntax().text_range().start()),
        flags,
        field_read,
        setter,
        return_reference,
        constructor::arity(method, &statements, ty),
    ))
}

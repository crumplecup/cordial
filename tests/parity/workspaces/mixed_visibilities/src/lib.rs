pub fn private_fn() {}

pub(crate) fn crate_fn() {}

pub fn public_fn() {}

#[tracing::instrument]
pub fn already_done() {}

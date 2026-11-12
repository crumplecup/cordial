//! Common body facts; configuration is evaluated after collection.
use super::ObservabilityBody;

impl ObservabilityBody {
    /// Record what a call of this function resolved to. A subscriber is installed
    /// by `tracing`'s own functions, not by anything that happens to be called
    /// `init`; a fallback is the standard library's `unwrap_or` family.
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn note_callee(&mut self, callee: &crate::ir::SemanticCallee) {
        const SUBSCRIBER: &[&str] = &["tracing_subscriber", "tracing", "tracing_core"];
        self.calls_install |= callee.is(SUBSCRIBER, &["init", "try_init", "set_global_default"]);
        self.calls_try_init |= callee.is(SUBSCRIBER, &["try_init", "set_global_default"]);
        self.has_try_from_default_env |=
            callee.is(&["tracing_subscriber"], &["try_from_default_env"]);
        self.has_once |= callee.is_std_method(
            &["Once", "OnceLock"],
            &[
                "call_once",
                "call_once_force",
                "get_or_init",
                "get_or_try_init",
                "set",
            ],
        );
        self.callees.push(callee.clone());
        self.has_fallback |= callee.is(
            &["core", "std"],
            &["unwrap_or", "unwrap_or_else", "unwrap_or_default"],
        );
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn rust_log_ok(&self) -> bool {
        (self.has_try_from_default_env || self.has_rust_log_literal) && self.has_fallback
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn idempotent_ok(&self) -> bool {
        self.calls_try_init || (self.has_once && self.calls_install)
    }

    /// Whether a call resolved to one of the `known` helpers. A configured path
    /// names the helper as its crate and function; the resolved definition may
    /// sit deeper in that crate behind a re-export, so a path with a crate
    /// matches the definition in that crate with that name, and a bare name
    /// matches any definition of it.
    #[tracing::instrument(level = "trace", skip(self, known))]
    pub(crate) fn calls_known_helper(&self, known: &[String]) -> bool {
        self.callees.iter().any(|callee| {
            known.iter().any(|helper| {
                let name = helper.rsplit("::").next().unwrap_or(helper);
                let krate = helper.split_once("::").map(|(krate, _)| krate);
                callee.path() == helper
                    || (callee.name() == name
                        && krate.is_none_or(|krate| callee.krate().as_deref() == Some(krate)))
            })
        })
    }

    #[tracing::instrument(level = "trace", skip(self, known))]
    pub(crate) fn installs_or_delegates(&self, known: &[String]) -> bool {
        self.calls_install || self.calls_known_helper(known)
    }

    #[tracing::instrument(level = "trace", skip(self, known))]
    pub(crate) fn reports_errors(&self, known: &[String]) -> bool {
        self.has_err_instrument || self.has_error_emission || self.calls_known_helper(known)
    }

    /// Whether this function calls any of the functions defined at `helpers`.
    #[tracing::instrument(level = "trace", skip(self, helpers))]
    pub(crate) fn calls_helper(&self, helpers: &[crate::FunctionLocation]) -> bool {
        self.called.iter().any(|called| helpers.contains(called))
    }
}

//! Common body facts; configuration is evaluated after collection.
use super::ObservabilityBody;

impl ObservabilityBody {
    #[tracing::instrument(level = "debug", skip(self))]
    pub(super) fn note_call(&mut self, name: &str, method: bool) {
        self.called_names.push(name.to_string());
        self.calls_install |= matches!(name, "init" | "try_init" | "set_global_default");
        self.calls_try_init |= matches!(name, "try_init" | "set_global_default");
        self.has_try_from_default_env |= name == "try_from_default_env";
        self.has_fallback |=
            method && matches!(name, "unwrap_or" | "unwrap_or_else" | "unwrap_or_default");
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub(super) fn note_path(&mut self, name: &str) {
        self.has_once |= matches!(name, "Once" | "OnceLock");
        self.has_try_from_default_env |= name == "try_from_default_env";
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn rust_log_ok(&self) -> bool {
        (self.has_try_from_default_env || self.has_rust_log_literal) && self.has_fallback
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn idempotent_ok(&self) -> bool {
        self.calls_try_init || (self.has_once && self.calls_install)
    }

    #[tracing::instrument(level = "trace", skip(self, known))]
    pub(crate) fn calls_known_helper(&self, known: &[String]) -> bool {
        self.called_paths.iter().any(|path| {
            let last = path.rsplit("::").next();
            known
                .iter()
                .any(|helper| helper == path || helper.rsplit("::").next() == last)
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

    #[tracing::instrument(level = "trace", skip(self, helpers))]
    pub(crate) fn calls_helper(&self, helpers: &[&str]) -> bool {
        self.called_names
            .iter()
            .any(|name| helpers.contains(&name.as_str()))
    }
}

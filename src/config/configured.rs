//! Required configuration identity for every etiquette and rule.

/// Identifies the etiquette table whose shared policy controls this object.
///
/// Both [`crate::Etiquette`] and [`crate::Rule`] require this trait, including
/// implementations supplied by plugins. The session applies configuration;
/// implementations do not need to implement toggles or path matching.
///
/// ```compile_fail
/// struct UnconfiguredRule;
/// impl cordial::Rule for UnconfiguredRule {
///     fn id(&self) -> &str { "CUSTOM-001" }
///     fn description(&self) -> &str { "A custom check" }
/// }
/// ```
pub trait TomlConfigured: Send + Sync {
    /// Stable owning etiquette id, shared by its rules and its configuration.
    fn configuration_key(&self) -> &str;
}

impl<T: TomlConfigured + ?Sized> TomlConfigured for &T {
    #[tracing::instrument(level = "trace", skip(self))]
    fn configuration_key(&self) -> &str {
        (**self).configuration_key()
    }
}

use crate::session::RunFilter;

use tracing::instrument;
/// Filter a session run to specific plugins, etiquettes, and/or one crate name.
#[derive(Debug, Default, Clone)]
pub struct NamedRunFilter {
    plugins: Vec<String>,
    etiquettes: Vec<String>,
    crate_name: Option<String>,
}

impl NamedRunFilter {
    /// All plugins.
    #[instrument(level = "debug")]
    pub fn all_plugins() -> Self {
        Self::default()
    }

    /// All etiquettes.
    #[instrument(level = "debug")]
    pub fn all_etiquettes() -> Self {
        Self::default()
    }

    /// Etiquettes registered on this session.
    #[instrument(level = "debug", skip(ids))]
    pub fn plugins(ids: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            plugins: ids.into_iter().map(|id| id.as_ref().to_string()).collect(),
            etiquettes: Vec::new(),
            crate_name: None,
        }
    }

    /// Etiquettes registered on this session.
    #[instrument(level = "debug", skip(ids))]
    pub fn etiquettes(ids: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            plugins: Vec::new(),
            etiquettes: ids.into_iter().map(|id| id.as_ref().to_string()).collect(),
            crate_name: None,
        }
    }

    /// Return a copy with `crate` set.
    #[instrument(level = "trace", skip(self, crate_name))]
    pub fn with_crate(mut self, crate_name: impl Into<String>) -> Self {
        self.crate_name = Some(crate_name.into());
        self
    }

    /// Package name this IR belongs to.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_name(&self) -> Option<&str> {
        self.crate_name.as_deref()
    }
}

impl RunFilter for NamedRunFilter {
    #[instrument(level = "trace", skip(self))]
    fn plugins(&self) -> Option<&[String]> {
        if self.plugins.is_empty() {
            None
        } else {
            Some(&self.plugins)
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> Option<&[String]> {
        if self.etiquettes.is_empty() {
            None
        } else {
            Some(&self.etiquettes)
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> Option<&str> {
        self.crate_name.as_deref()
    }
}

use crate::session::RunFilter;

use tracing::instrument;
/// Filter a session run to specific plugins, etiquettes, and/or one crate name.
#[derive(Debug, Default, Clone)]
pub struct NamedRunFilter {
    plugins: Vec<&'static str>,
    etiquettes: Vec<&'static str>,
    crate_name: Option<String>,
}

impl NamedRunFilter {
    #[instrument(level = "debug")]
    pub fn all_plugins() -> Self {
        Self::default()
    }

    #[instrument(level = "debug")]
    pub fn all_etiquettes() -> Self {
        Self::default()
    }

    #[instrument(level = "debug")]
    pub fn plugins(ids: &'static [&'static str]) -> Self {
        Self {
            plugins: ids.to_vec(),
            etiquettes: Vec::new(),
            crate_name: None,
        }
    }

    #[instrument(level = "debug")]
    pub fn etiquettes(ids: &'static [&'static str]) -> Self {
        Self {
            plugins: Vec::new(),
            etiquettes: ids.to_vec(),
            crate_name: None,
        }
    }

    #[instrument(level = "trace", skip(self, crate_name))]
    pub fn with_crate(mut self, crate_name: impl Into<String>) -> Self {
        self.crate_name = Some(crate_name.into());
        self
    }

    #[instrument(level = "trace", skip(self))]
    pub fn crate_name(&self) -> Option<&str> {
        self.crate_name.as_deref()
    }
}

impl RunFilter for NamedRunFilter {
    fn plugins(&self) -> Option<&[&str]> {
        if self.plugins.is_empty() {
            None
        } else {
            Some(&self.plugins)
        }
    }

    fn etiquettes(&self) -> Option<&[&str]> {
        if self.etiquettes.is_empty() {
            None
        } else {
            Some(&self.etiquettes)
        }
    }

    fn crate_name(&self) -> Option<&str> {
        self.crate_name.as_deref()
    }
}

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};
use crate::loader::LoadView;

use super::scan::survey_crate_dependency_freshness;
use super::types::DependencySurveyRecord;

use tracing::instrument;

/// Loads manifest and lockfile dependency freshness survey data.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessLoader;

impl DependencyFreshnessLoader {
    /// Stable loader id.
    pub const ID: &'static str = "dependency-freshness";
}

impl Loader for DependencyFreshnessLoader {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>> {
        let target = view.target;
        let records = survey_crate_dependency_freshness(
            view.session.project_root(),
            target.crate_root(),
            target.crate_name(),
            Some(view.session.store_root()),
        )?;
        Ok(Box::new(DependencyFreshnessLoadView::new(
            target.crate_name().clone(),
            records,
        )))
    }
}

/// Dependency freshness survey loader output.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct DependencyFreshnessLoadView {
    crate_name: String,
    records: Vec<DependencySurveyRecord>,
}

impl LoadView for DependencyFreshnessLoadView {
    #[instrument(level = "trace", skip(self))]
    fn loader_id(&self) -> &str {
        DependencyFreshnessLoader::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

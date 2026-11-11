//! Project antipattern source inventories from canonical database snapshots.
use super::SourceDatabase;
use crate::{CordialResult, SourceAntipatterns};

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_antipatterns(
        &self,
        target: &crate::CrateTarget,
    ) -> CordialResult<SourceAntipatterns> {
        let root = target.crate_root();
        let contracts =
            crate::etiquettes::antipatterns::antipattern_contract_verifier(target.crate_name())
                .is_some();
        let paths = crate::etiquettes::antipatterns::native_input_paths(root, contracts)?;
        let inputs: Vec<_> = paths.iter().map(|(_, path)| path.clone()).collect();
        self.collect_sources(&inputs, |sources| {
            let files: Vec<_> = paths
                .iter()
                .zip(sources)
                .map(|((tree, _), source)| (tree.clone(), source))
                .collect();
            SourceAntipatterns::collect(root, target.crate_name(), &files)
        })
    }
}

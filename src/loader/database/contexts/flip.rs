//! A cfg context as the ordinary crates under changed cfg, flipped in place.
//!
//! A context used to be copies of the crates under their own files (`context_crates`), so that what the ordinary
//! build computed stayed valid beside it. Rust-analyzer now asks the cfg one atom at a time and a finished
//! definition map is read by module (`docs/planning/contexts-as-flips.md`), so a crate's own cfg can change and
//! only what read a flipped atom runs again. A context writes its change, is read from the same crates and files,
//! and the ordinary cfg is written back.

use super::LocalCfgs;
use crate::SemanticContextKey;
use crate::loader::database::SourceDatabase;
use crate::loader::database::context_crates::ContextCrates;
use ra_ap_cfg::CfgOptions;
use ra_ap_ide_db::base_db::Crate as InputCrate;
use salsa::Durability;
use salsa::Setter as _;
use tracing::instrument;

impl SourceDatabase {
    /// Give each local crate the cfg `context` makes of its ordinary one, and say which crates they are.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn flip_context(
        &mut self,
        cfgs: &LocalCfgs,
        context: &SemanticContextKey,
    ) -> ContextCrates {
        let pairs = cfgs.pairs(context);
        let mut flipped = 0usize;
        let mut crates = Vec::new();
        for ((krate, _), (_, changed)) in cfgs.cfgs().zip(pairs) {
            flipped += usize::from(self.write_cfg(*krate, changed));
            crates.push(*krate);
        }
        tracing::info!(
            target: "cordial::semantic::contexts",
            %context,
            crates = crates.len() as u64,
            flipped = flipped as u64,
            "a context's cfg is written into the ordinary crates"
        );
        ContextCrates::in_place(context.clone(), crates)
    }

    /// Put each local crate's ordinary cfg back.
    #[instrument(level = "debug", skip(self, cfgs))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn restore_cfgs(&mut self, cfgs: &LocalCfgs) {
        for (krate, ordinary) in cfgs.cfgs() {
            self.write_cfg(*krate, ordinary.clone());
        }
    }

    /// Write `options` as the crate's cfg when it is not what it holds: an input is not compared to its old value,
    /// so writing an equal one would make everything that read it stale. Whether it was written.
    fn write_cfg(&mut self, krate: InputCrate, options: CfgOptions) -> bool {
        if *krate.cfg_options(&self.db) == options {
            return false;
        }
        krate
            .set_cfg_options(&mut self.db)
            .with_durability(Durability::LOW)
            .to(options);
        true
    }
}

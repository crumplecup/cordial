//! `cordial base`: the state a project with no state of its own starts from.

use std::path::Path;

use clap::Subcommand;
use tracing::instrument;

use crate::{CordialResult, SysrootCache, default_store_home};

/// What to do with the base state.
#[derive(Subcommand)]
pub enum BaseCommands {
    /// Make this build's base state if there is none: the sysroot, built and saved once.
    Build,
    /// Say whether this build has a base state, and where it is.
    Status,
}

impl BaseCommands {
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(super) fn act(self, store_home: Option<&Path>) -> CordialResult<()> {
        let sysroot =
            SysrootCache::from_home(store_home.map_or_else(default_store_home, Path::to_path_buf));
        let directory = crate::base_directory(&sysroot);
        match self {
            Self::Build => {
                let started = std::time::Instant::now();
                let ready = crate::base_is_ready(&directory);
                let directory = crate::base_state::build(&sysroot)?;
                println!(
                    "base state {} at {}{}",
                    if ready { "was already there" } else { "made" },
                    directory.display(),
                    if ready {
                        String::new()
                    } else {
                        format!(" in {:?}", started.elapsed())
                    }
                );
            }
            Self::Status => {
                if crate::base_is_ready(&directory) {
                    println!("base state: {}", directory.display());
                } else {
                    println!(
                        "base state: none for this build (it is made when a project has no state of its own)"
                    );
                }
            }
        }
        Ok(())
    }
}

//! Resolve fixture workspaces under `tests/parity/workspaces/`.

use std::path::PathBuf;

fn parity_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity")
}

pub fn workspace_path(name: &str) -> PathBuf {
    parity_root().join("workspaces").join(name)
}

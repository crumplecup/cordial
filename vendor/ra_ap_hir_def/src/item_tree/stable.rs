//! What a tree id says, hashed so that it means the same in every process.

use hir_expand::stable::{StableCx, StableHash};

use super::TreeId;

impl StableHash for TreeId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { file, block } = self;
        file.stable_hash(cx);
        block.stable_hash(cx);
    }
}

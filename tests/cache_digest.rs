use std::path::{Path, PathBuf};

use cordial::IrCacheDigest;

#[test]
fn digest_path_uses_crate_name() {
    let path = IrCacheDigest::cache_path(Path::new("/tmp/cache"), "demo");
    assert_eq!(path, PathBuf::from("/tmp/cache/demo.ir.digests.json"));
}

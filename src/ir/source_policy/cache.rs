//! A decoded inventory shared during enrichment, alongside its original wire bytes.

use super::SourcePolicyInventory;
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub(crate) struct Cache(OnceLock<Arc<SourcePolicyInventory>>);

impl Clone for Cache {
    fn clone(&self) -> Self {
        // Graph copies share encoded bytes, never decoded caches.
        Self::default()
    }
}

impl Cache {
    pub(crate) fn get(&self) -> Option<Arc<SourcePolicyInventory>> {
        self.0.get().map(Arc::clone)
    }

    pub(crate) fn insert(&self, inventory: SourcePolicyInventory) -> Arc<SourcePolicyInventory> {
        Arc::clone(self.0.get_or_init(|| Arc::new(inventory)))
    }

    pub(crate) fn clear(&mut self) -> bool {
        self.0.take().is_some()
    }

    pub(crate) fn is_decoded(&self) -> bool {
        self.0.get().is_some()
    }
}

use std::{collections::HashSet, sync::Arc};

use crate::model::AssetKey;

/// The AssetKeys this node asks for.
///
/// A want is local state: no message carries it, because a lookup asks the nodes closest to the key directly.
pub struct WantCache {
    keys: HashSet<Arc<AssetKey>>,
}

impl WantCache {
    /// The most AssetKeys one node asks for at a time.
    pub const MAX_ASSET_KEYS: usize = 4096;

    pub fn new() -> Self {
        Self { keys: HashSet::new() }
    }

    /// Replaces the AssetKeys the higher layer asks for.
    pub fn set_local(&mut self, keys: impl IntoIterator<Item = Arc<AssetKey>>) {
        self.keys = keys.into_iter().take(Self::MAX_ASSET_KEYS).collect();
    }

    /// The AssetKeys the lookup asks the network for.
    #[allow(dead_code)]
    pub fn keys(&self) -> Vec<Arc<AssetKey>> {
        self.keys.iter().cloned().collect()
    }
}

impl Default for WantCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::*;

    fn key(name: &str) -> Arc<AssetKey> {
        Arc::new(AssetKey {
            typ: name.to_owned(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: vec![1; 32],
            },
        })
    }

    #[test]
    fn the_local_wants_are_replaced_as_a_whole() {
        let mut cache = WantCache::new();
        cache.set_local([key("one"), key("two")]);
        assert_eq!(cache.keys().len(), 2);

        cache.set_local([key("three")]);
        assert_eq!(cache.keys(), vec![key("three")]);
    }

    #[test]
    fn at_most_the_limit_is_kept() {
        let mut cache = WantCache::new();
        cache.set_local((0..WantCache::MAX_ASSET_KEYS + 1).map(|i| key(&format!("{i:05}"))));
        assert_eq!(cache.keys().len(), WantCache::MAX_ASSET_KEYS);
    }
}

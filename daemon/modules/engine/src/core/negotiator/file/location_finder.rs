use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::{
    base::sync::FnHandle,
    core::negotiator::NodeFinder,
    model::{AssetKey, NodeProfile},
    prelude::*,
};

/// Where FileExchanger advertises the root hashes it provides and finds the nodes holding a root hash.
#[async_trait]
pub trait LocationFinder: Send + Sync {
    fn my_node_id(&self) -> Vec<u8>;

    /// Replaces the asset keys that are advertised by push and requested by want.
    fn set_asset_keys(&self, asset_keys: Vec<AssetKey>);

    async fn find(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>>;
}

/// Advertises through NodeFinder. The kept handles register the keys until this value is dropped.
pub struct NodeFinderLocationFinder {
    node_finder: Arc<NodeFinder>,
    asset_keys: Arc<Mutex<Vec<AssetKey>>>,
    _push_handle: FnHandle<Vec<AssetKey>, ()>,
    _want_handle: FnHandle<Vec<AssetKey>, ()>,
}

impl NodeFinderLocationFinder {
    pub fn new(node_finder: Arc<NodeFinder>) -> Self {
        let asset_keys = Arc::new(Mutex::new(Vec::new()));
        let push_handle = node_finder.listen_push_asset_keys({
            let asset_keys = asset_keys.clone();
            move |_| asset_keys.lock().clone()
        });
        let want_handle = node_finder.listen_want_asset_keys({
            let asset_keys = asset_keys.clone();
            move |_| asset_keys.lock().clone()
        });
        Self {
            node_finder,
            asset_keys,
            _push_handle: push_handle,
            _want_handle: want_handle,
        }
    }
}

#[async_trait]
impl LocationFinder for NodeFinderLocationFinder {
    fn my_node_id(&self) -> Vec<u8> {
        self.node_finder.my_node_profile().id().to_vec()
    }

    fn set_asset_keys(&self, asset_keys: Vec<AssetKey>) {
        *self.asset_keys.lock() = asset_keys;
    }

    async fn find(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        self.node_finder.find_node_profile(asset_key).await
    }
}

/// Returns the locations a test sets, and remembers the asset keys FileExchanger advertised.
#[cfg(test)]
pub struct FixedLocationFinder {
    my_node_id: Vec<u8>,
    locations: Mutex<Vec<Arc<NodeProfile>>>,
    asset_keys: Mutex<Vec<AssetKey>>,
}

#[cfg(test)]
impl FixedLocationFinder {
    pub fn new(my_node_id: Vec<u8>) -> Self {
        Self {
            my_node_id,
            locations: Mutex::new(Vec::new()),
            asset_keys: Mutex::new(Vec::new()),
        }
    }

    pub fn set_locations(&self, node_profiles: Vec<NodeProfile>) {
        *self.locations.lock() = node_profiles.into_iter().map(Arc::new).collect();
    }

    pub fn asset_keys(&self) -> Vec<AssetKey> {
        self.asset_keys.lock().clone()
    }
}

#[cfg(test)]
#[async_trait]
impl LocationFinder for FixedLocationFinder {
    fn my_node_id(&self) -> Vec<u8> {
        self.my_node_id.clone()
    }

    fn set_asset_keys(&self, asset_keys: Vec<AssetKey>) {
        *self.asset_keys.lock() = asset_keys;
    }

    async fn find(&self, _asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        Ok(self.locations.lock().clone())
    }
}

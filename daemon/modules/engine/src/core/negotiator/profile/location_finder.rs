use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::{
    base::sync::FnHandle,
    core::negotiator::NodeFinder,
    model::{AssetKey, NodeProfile},
    prelude::*,
};

/// Where ProfileExchanger advertises the Profiles it owns and finds the nodes that hold a Profile.
#[async_trait]
pub trait ProfileLocationFinder: Send + Sync {
    fn my_node_id(&self) -> Vec<u8>;

    /// Replaces the AssetKeys of the Profiles this node owns.
    fn set_provided(&self, asset_keys: Vec<AssetKey>);

    async fn find(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>>;
}

/// Advertises through NodeFinder. The kept handle registers the keys until this value is dropped.
pub struct NodeFinderProfileLocationFinder {
    node_finder: Arc<NodeFinder>,
    provided: Arc<Mutex<Vec<AssetKey>>>,
    _push_handle: FnHandle<Vec<AssetKey>, ()>,
}

impl NodeFinderProfileLocationFinder {
    pub fn new(node_finder: Arc<NodeFinder>) -> Self {
        let provided = Arc::new(Mutex::new(Vec::new()));
        let push_handle = node_finder.listen_push_asset_keys({
            let provided = provided.clone();
            move |_| provided.lock().clone()
        });
        Self {
            node_finder,
            provided,
            _push_handle: push_handle,
        }
    }
}

#[async_trait]
impl ProfileLocationFinder for NodeFinderProfileLocationFinder {
    fn my_node_id(&self) -> Vec<u8> {
        self.node_finder.my_node_profile().id().to_vec()
    }

    fn set_provided(&self, asset_keys: Vec<AssetKey>) {
        *self.provided.lock() = asset_keys;
    }

    async fn find(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        self.node_finder.lookup(asset_key).await
    }
}

/// Returns the locations a test sets, and remembers the Profile AssetKeys advertised by this node.
#[cfg(test)]
pub struct FixedProfileLocationFinder {
    my_node_id: Vec<u8>,
    locations: Mutex<Vec<Arc<NodeProfile>>>,
    provided: Mutex<Vec<AssetKey>>,
}

#[cfg(test)]
impl FixedProfileLocationFinder {
    pub fn new(my_node_id: Vec<u8>) -> Self {
        Self {
            my_node_id,
            locations: Mutex::new(Vec::new()),
            provided: Mutex::new(Vec::new()),
        }
    }

    pub fn set_locations(&self, node_profiles: Vec<NodeProfile>) {
        *self.locations.lock() = node_profiles.into_iter().map(Arc::new).collect();
    }

    pub fn provided(&self) -> Vec<AssetKey> {
        self.provided.lock().clone()
    }
}

#[cfg(test)]
#[async_trait]
impl ProfileLocationFinder for FixedProfileLocationFinder {
    fn my_node_id(&self) -> Vec<u8> {
        self.my_node_id.clone()
    }

    fn set_provided(&self, asset_keys: Vec<AssetKey>) {
        *self.provided.lock() = asset_keys;
    }

    async fn find(&self, _asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        Ok(self.locations.lock().clone())
    }
}

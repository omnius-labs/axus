use std::{collections::HashMap, sync::Arc};

use chrono::{Duration, Utc};
use parking_lot::Mutex;

use omnius_core_base::clock::Clock;

use crate::{
    base::collections::{VolatileHashMap, VolatileHashSet},
    core::session::model::Session,
    model::{AssetKey, NodeProfile},
};

#[derive(Clone)]
pub struct SessionStatus {
    pub session: Session,
    pub node_profile: Arc<Mutex<Option<NodeProfile>>>,
    pub sending_data_message: Arc<Mutex<SendingDataMessage>>,
    pub received_data_message: Arc<Mutex<ReceivedDataMessage>>,
}

impl SessionStatus {
    pub fn new(session: Session, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            session,
            node_profile: Arc::new(Mutex::new(None)),
            sending_data_message: Arc::new(Mutex::new(SendingDataMessage::new())),
            received_data_message: Arc::new(Mutex::new(ReceivedDataMessage::new(clock))),
        }
    }
}

pub struct SendingDataMessage {
    pub push_node_profiles: Vec<Arc<NodeProfile>>,
    pub want_asset_keys: Vec<Arc<AssetKey>>,
    pub give_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
    pub push_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
}

impl SendingDataMessage {
    pub fn new() -> Self {
        Self {
            push_node_profiles: vec![],
            want_asset_keys: vec![],
            give_asset_key_locations: HashMap::new(),
            push_asset_key_locations: HashMap::new(),
        }
    }
}

impl Default for SendingDataMessage {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ReceivedDataMessage {
    pub want_asset_keys: VolatileHashSet<Arc<AssetKey>>,
    pub give_asset_key_locations: VolatileHashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
    pub push_asset_key_locations: VolatileHashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
}

impl ReceivedDataMessage {
    pub fn new(clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            want_asset_keys: VolatileHashSet::new(Duration::minutes(30), clock.clone()),
            give_asset_key_locations: VolatileHashMap::new(Duration::minutes(30), clock.clone()),
            push_asset_key_locations: VolatileHashMap::new(Duration::minutes(30), clock),
        }
    }
}

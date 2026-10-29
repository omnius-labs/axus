use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    core::session::model::Session,
    model::{AssetKey, NodeProfile},
};

#[derive(Clone)]
pub struct SessionStatus {
    pub session: Session,
    pub node_profile: Arc<Mutex<Option<NodeProfile>>>,
    pub sending_data_message: Arc<Mutex<SendingDataMessage>>,
    /// 同じ相手との重複を解消するとき、この Session の送受信だけを止める
    pub cancellation_token: CancellationToken,
}

impl SessionStatus {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            node_profile: Arc::new(Mutex::new(None)),
            sending_data_message: Arc::new(Mutex::new(SendingDataMessage::new())),
            cancellation_token: CancellationToken::new(),
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

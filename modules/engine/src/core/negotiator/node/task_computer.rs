use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use futures::FutureExt;
use parking_lot::Mutex;
use rand::seq::SliceRandom as _;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::sleeper::Sleeper;

use crate::{
    base::{Shutdown, sync::FnCaller},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskComputer {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    node_profile_repo: Arc<NodeFinderRepo>,
    node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    get_want_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
    get_push_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    #[allow(unused)]
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl Shutdown for TaskComputer {
    async fn shutdown(&self) {
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

impl TaskComputer {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        node_profile_repo: Arc<NodeFinderRepo>,
        node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
        get_want_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
        get_push_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            my_node_profile,
            node_profile_repo,
            node_profile_fetcher,
            sessions,
            get_want_asset_keys_fn,
            get_push_asset_keys_fn,
            sleeper,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            if let Err(e) = this.set_initial_node_profile().await {
                warn!(error_message = e.to_string(), "set initial node profile failed");
            }
            loop {
                this.sleeper.sleep(std::time::Duration::from_secs(60)).await;
                let res = this.compute().await;
                if let Err(e) = res {
                    warn!(error_message = e.to_string(), "compute failed");
                }
            }
        }));

        Ok(())
    }

    pub async fn set_initial_node_profile(&self) -> Result<()> {
        let node_profiles = self.node_profile_fetcher.fetch().await?;
        let node_profiles: Vec<&NodeProfile> = node_profiles.iter().collect();
        self.node_profile_repo.insert_or_ignore_node_profiles(&node_profiles, 0).await?;

        Ok(())
    }

    pub async fn compute(&self) -> Result<()> {
        self.compute_sending_data_message().await?;

        Ok(())
    }

    #[allow(clippy::type_complexity)]
    async fn compute_sending_data_message(&self) -> Result<()> {
        let my_node_profile = Arc::new(self.my_node_profile.lock().clone());
        let cloud_node_profile: Vec<Arc<NodeProfile>> = self.node_profile_repo.fetch_node_profiles().await?.into_iter().map(Arc::new).collect();

        let my_get_want_asset_keys: HashSet<Arc<AssetKey>> = self.get_want_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();
        let my_get_push_asset_keys: HashSet<Arc<AssetKey>> = self.get_push_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();

        let mut received_data_map: HashMap<Vec<u8>, ReceivedTempDataMessage> = HashMap::new();
        {
            let sessions = self.sessions.read().await;
            for (id, status) in sessions.iter() {
                let data = status.received_data_message.lock();

                let mut want_asset_keys: Vec<Arc<AssetKey>> = data.want_asset_keys.iter().cloned().collect();
                let mut give_asset_key_locations: Vec<(Arc<AssetKey>, Vec<Arc<NodeProfile>>)> =
                    data.give_asset_key_locations.iter().map(|(k, v)| (k.clone(), v.to_vec())).collect();
                let mut push_asset_key_locations: Vec<(Arc<AssetKey>, Vec<Arc<NodeProfile>>)> =
                    data.push_asset_key_locations.iter().map(|(k, v)| (k.clone(), v.to_vec())).collect();

                let mut rng = rand::rng();
                want_asset_keys.shuffle(&mut rng);
                give_asset_key_locations.shuffle(&mut rng);
                push_asset_key_locations.shuffle(&mut rng);

                let tmp = ReceivedTempDataMessage {
                    want_asset_keys,
                    give_asset_key_locations,
                    push_asset_key_locations,
                };
                received_data_map.insert(id.clone(), tmp);
            }
        }

        let ids: Vec<&[u8]> = received_data_map.keys().map(|n| n.as_slice()).collect();

        // 全ノードに配布する情報
        let mut push_node_profiles: HashSet<Arc<NodeProfile>> = HashSet::new();
        push_node_profiles.insert(my_node_profile.clone());
        push_node_profiles.extend(cloud_node_profile);

        // Kadexの距離が近いノードに配布する情報
        let mut want_asset_keys: HashSet<Arc<AssetKey>> = HashSet::new();
        want_asset_keys.extend(my_get_want_asset_keys);
        for data in received_data_map.values() {
            want_asset_keys.extend(data.want_asset_keys.iter().cloned());
        }

        // Wantリクエストを受けたノードに配布する情報
        let mut give_asset_key_locations: HashMap<Arc<AssetKey>, HashSet<Arc<NodeProfile>>> = HashMap::new();
        for asset_key in my_get_push_asset_keys.iter() {
            give_asset_key_locations.entry(asset_key.clone()).or_default().insert(my_node_profile.clone());
        }
        for data in received_data_map.values() {
            let iter1 = data.push_asset_key_locations.iter();
            let iter2 = data.give_asset_key_locations.iter();
            for (asset_key, node_profiles) in iter1.chain(iter2) {
                give_asset_key_locations.entry(asset_key.clone()).or_default().extend(node_profiles.iter().cloned());
            }
        }

        // Kadexの距離が近いノードに配布する情報
        let mut push_asset_key_locations: HashMap<Arc<AssetKey>, HashSet<Arc<NodeProfile>>> = HashMap::new();
        for asset_key in my_get_push_asset_keys.iter() {
            push_asset_key_locations.entry(asset_key.clone()).or_default().insert(my_node_profile.clone());
        }
        for data in received_data_map.values() {
            for (asset_key, node_profiles) in data.push_asset_key_locations.iter() {
                give_asset_key_locations.entry(asset_key.clone()).or_default().extend(node_profiles.iter().cloned());
            }
        }

        // Kadexの距離が近いノードにwant_asset_keyを配布する
        let mut sending_want_asset_key_map: HashMap<&[u8], Vec<Arc<AssetKey>>> = HashMap::new();
        for target_key in want_asset_keys.iter() {
            for id in Kadex::find(&my_node_profile.id, &target_key.hash.value, &ids, 1) {
                sending_want_asset_key_map.entry(id).or_default().push(target_key.clone());
            }
        }

        // want_asset_keyを受け取ったノードにgive_asset_key_locationsを配布する
        let mut sending_give_asset_key_location_map: HashMap<&[u8], HashMap<Arc<AssetKey>, &HashSet<Arc<NodeProfile>>>> = HashMap::new();
        for (id, data) in received_data_map.iter() {
            for target_key in data.want_asset_keys.iter() {
                if let Some((target_key, node_profiles)) = give_asset_key_locations.get_key_value(target_key) {
                    sending_give_asset_key_location_map.entry(id).or_default().insert(target_key.clone(), node_profiles);
                }
            }
        }

        // Kadexの距離が近いノードにpush_asset_key_locationsを配布する
        let mut sending_push_asset_key_location_map: HashMap<&[u8], HashMap<Arc<AssetKey>, &HashSet<Arc<NodeProfile>>>> = HashMap::new();
        for (target_key, node_profiles) in push_asset_key_locations.iter() {
            for id in Kadex::find(&my_node_profile.id, &target_key.hash.value, &ids, 1) {
                sending_push_asset_key_location_map.entry(id).or_default().insert(target_key.clone(), node_profiles);
            }
        }

        // Session毎にデータを実体化する
        let mut sending_data_map: HashMap<Vec<u8>, SendingDataMessage> = HashMap::new();

        let push_node_profiles: Vec<Arc<NodeProfile>> = push_node_profiles.into_iter().collect();

        for id in received_data_map.keys() {
            let want_asset_keys = sending_want_asset_key_map
                .get(id.as_slice())
                .unwrap_or(&Vec::new())
                .iter()
                .take(1024 * 256)
                .cloned()
                .collect();
            let give_asset_key_locations = sending_give_asset_key_location_map
                .get(id.as_slice())
                .unwrap_or(&HashMap::new())
                .iter()
                .take(1024 * 256)
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect();
            let push_asset_key_locations = sending_push_asset_key_location_map
                .get(id.as_slice())
                .unwrap_or(&HashMap::new())
                .iter()
                .take(1024 * 256)
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect();

            let data_message = SendingDataMessage {
                push_node_profiles: push_node_profiles.clone(),
                want_asset_keys,
                give_asset_key_locations,
                push_asset_key_locations,
            };
            sending_data_map.insert(id.clone(), data_message);
        }

        // Session毎に送信用データを格納する
        {
            let mut sessions = self.sessions.write().await;
            for (id, status) in sessions.iter_mut() {
                if let Some(data_message) = sending_data_map.remove(id) {
                    *status.sending_data_message.lock() = data_message;
                }
            }
        }

        Ok(())
    }
}

struct ReceivedTempDataMessage {
    pub want_asset_keys: Vec<Arc<AssetKey>>,
    pub give_asset_key_locations: Vec<(Arc<AssetKey>, Vec<Arc<NodeProfile>>)>,
    pub push_asset_key_locations: Vec<(Arc<AssetKey>, Vec<Arc<NodeProfile>>)>,
}

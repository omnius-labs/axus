use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use parking_lot::Mutex;
use rand::seq::SliceRandom as _;
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use omnius_core_base::sleeper::Sleeper;

use crate::{
    base::{runtime::Shutdown, sync::FnCaller},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskComputer {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    node_profile_repo: Arc<NodeFinderRepo>,
    node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
    sessions: Arc<SessionRegistry>,
    location_cache: Arc<Mutex<LocationCache>>,
    want_cache: Arc<Mutex<WantCache>>,
    get_want_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
    get_push_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskComputer {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }
    }
}

impl TaskComputer {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        node_profile_repo: Arc<NodeFinderRepo>,
        node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
        sessions: Arc<SessionRegistry>,
        location_cache: Arc<Mutex<LocationCache>>,
        want_cache: Arc<Mutex<WantCache>>,
        get_want_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
        get_push_asset_keys_fn: FnCaller<Vec<AssetKey>, ()>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            my_node_profile,
            node_profile_repo,
            node_profile_fetcher,
            sessions,
            location_cache,
            want_cache,
            get_want_asset_keys_fn,
            get_push_asset_keys_fn,
            sleeper,
            rng,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
            token: CancellationToken::new(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            select! {
                res = this.set_initial_node_profile() => {
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "set initial node profile failed");
                    }
                }
                _ = this.token.cancelled() => {}
            }
            while !this.token.is_cancelled() {
                select! {
                    _ = async {
                        this.sleeper.sleep(this.option.intervals.compute).await;
                        let res = this.compute().await;
                        if let Err(e) = res {
                            warn!(error_message = e.to_string(), "compute failed");
                        }
                    } => {}
                    _ = this.token.cancelled() => break,
                }
            }
        }));

        Ok(())
    }

    pub async fn set_initial_node_profile(&self) -> Result<()> {
        let node_profiles = self.node_profile_fetcher.fetch().await?;
        let node_profiles: Vec<&NodeProfile> = node_profiles.iter().collect();
        self.node_profile_repo.insert_or_update_node_profiles(&node_profiles, 0).await?;

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

        let my_want_asset_keys: Vec<Arc<AssetKey>> = self.get_want_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();
        let my_push_asset_keys: HashSet<Arc<AssetKey>> = self.get_push_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();

        let ids: Vec<Vec<u8>> = self.sessions.ids().await;
        let id_slices: Vec<&[u8]> = ids.iter().map(|id| id.as_slice()).collect();

        // want と、Session ごとに答えられる want を 1 回の lock で集める
        let (want_asset_keys, relayed_want_asset_keys, wanted_by_session) = {
            let mut want_cache = self.want_cache.lock();
            want_cache.set_local(my_want_asset_keys);
            let wanted_by_session: HashMap<Vec<u8>, Vec<Arc<AssetKey>>> = ids.iter().map(|id| (id.clone(), want_cache.relayed_keys_of(id))).collect();
            (want_cache.keys(), want_cache.relayed_keys(), wanted_by_session)
        };

        // 全ノードに配布する情報
        let mut push_node_profiles: HashSet<Arc<NodeProfile>> = HashSet::new();
        push_node_profiles.insert(my_node_profile.clone());
        push_node_profiles.extend(cloud_node_profile);

        // 自分が提供する key と、受信済みの広告を答える候補にする
        let (give_asset_key_locations, push_asset_key_locations) = {
            let location_cache = self.location_cache.lock();
            let mut give: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>> = HashMap::new();
            let mut push: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>> = HashMap::new();
            for asset_key in my_push_asset_keys.iter() {
                give.entry(asset_key.clone()).or_default().push(my_node_profile.clone());
                push.entry(asset_key.clone()).or_default().push(my_node_profile.clone());
            }
            for asset_key in relayed_want_asset_keys.iter() {
                Self::extend_locations(give.entry(asset_key.clone()).or_default(), location_cache.get(asset_key).iter().cloned());
            }
            for (asset_key, locations) in location_cache.advertisements() {
                Self::extend_locations(push.entry(asset_key.clone()).or_default(), locations.iter().cloned());
            }
            (give, push)
        };

        // Kadex の距離が近いノードに want を配布する
        let mut sending_want_asset_key_map: HashMap<Vec<u8>, Vec<Arc<AssetKey>>> = HashMap::new();
        for target_key in want_asset_keys.iter() {
            for id in Kadex::find(my_node_profile.id(), &target_key.hash.value, &id_slices, 1) {
                sending_want_asset_key_map.entry(id.to_vec()).or_default().push(target_key.clone());
            }
        }

        // Kadex の距離が近いノードに広告を配布する
        let mut sending_push_asset_key_location_map: HashMap<Vec<u8>, HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = HashMap::new();
        for (target_key, node_profiles) in push_asset_key_locations.iter() {
            for id in Kadex::find(my_node_profile.id(), &target_key.hash.value, &id_slices, 1) {
                sending_push_asset_key_location_map
                    .entry(id.to_vec())
                    .or_default()
                    .insert(target_key.clone(), node_profiles.clone());
            }
        }

        // want を送ってきたノードにだけ、その key の所在を返す
        let mut sending_give_asset_key_location_map: HashMap<Vec<u8>, HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = HashMap::new();
        for (id, wanted) in wanted_by_session.iter() {
            for asset_key in wanted.iter() {
                if let Some(node_profiles) = give_asset_key_locations.get(asset_key) {
                    sending_give_asset_key_location_map
                        .entry(id.clone())
                        .or_default()
                        .insert(asset_key.clone(), node_profiles.clone());
                }
            }
        }

        // Session毎にデータを実体化する
        let mut sending_data_map: HashMap<Vec<u8>, SendingDataMessage> = HashMap::new();

        let push_node_profiles = self.select_node_profiles(push_node_profiles.into_iter().collect(), DataMessage::MAX_PUSH_NODE_PROFILES);

        for id in ids.iter() {
            let mut want_asset_keys = sending_want_asset_key_map.remove(id.as_slice()).unwrap_or_default();
            want_asset_keys.shuffle(&mut self.rng.lock());
            want_asset_keys.truncate(DataMessage::MAX_WANT_ASSET_KEYS);
            let give_asset_key_locations = self.select_asset_key_locations(
                sending_give_asset_key_location_map.remove(id.as_slice()).unwrap_or_default(),
                DataMessage::MAX_GIVE_ASSET_KEY_LOCATIONS,
            );
            let push_asset_key_locations = self.select_asset_key_locations(
                sending_push_asset_key_location_map.remove(id.as_slice()).unwrap_or_default(),
                DataMessage::MAX_PUSH_ASSET_KEY_LOCATIONS,
            );

            let data_message = SendingDataMessage {
                push_node_profiles: push_node_profiles.clone(),
                want_asset_keys,
                give_asset_key_locations,
                push_asset_key_locations,
            };
            let data_message = DataMessageBudget::select(data_message, &mut *self.rng.lock());
            sending_data_map.insert(id.clone(), data_message);
        }

        // Session毎に送信用データを格納する
        for (id, status) in self.sessions.statuses().await {
            if let Some(data_message) = sending_data_map.remove(&id) {
                *status.sending_data_message.lock() = data_message;
            }
        }

        Ok(())
    }

    /// Adds the locations that are not known yet, keeping the wire limit of one AssetKey.
    fn extend_locations(known: &mut Vec<Arc<NodeProfile>>, locations: impl IntoIterator<Item = Arc<NodeProfile>>) {
        for location in locations {
            if known.len() >= DataMessage::MAX_LOCATION_NODE_PROFILES {
                break;
            }
            if known.iter().any(|value| value.id() == location.id()) {
                continue;
            }
            known.push(location);
        }
    }

    fn select_node_profiles(&self, mut profiles: Vec<Arc<NodeProfile>>, max_count: usize) -> Vec<Arc<NodeProfile>> {
        profiles.shuffle(&mut *self.rng.lock());
        profiles.truncate(max_count);
        profiles
    }

    fn select_asset_key_locations(&self, locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>, max_count: usize) -> HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>> {
        let mut locations: Vec<(Arc<AssetKey>, Vec<Arc<NodeProfile>>)> = locations.into_iter().collect();
        locations.shuffle(&mut self.rng.lock());
        locations.truncate(max_count);
        locations
            .into_iter()
            .map(|(key, profiles)| (key, self.select_node_profiles(profiles, DataMessage::MAX_LOCATION_NODE_PROFILES)))
            .collect()
    }
}

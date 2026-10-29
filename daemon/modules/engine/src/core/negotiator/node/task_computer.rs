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
    core::negotiator::node::{AnnounceLocation, MAX_HOPS, NodeMessage},
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
        self.announce().await
    }

    /// Sends the known nodes to every Session and the AssetKeys this node provides to the Sessions closest to them.
    async fn announce(&self) -> Result<()> {
        let my_node_profile = Arc::new(self.my_node_profile.lock().clone());
        self.node_profile_repo.shrink(1024).await?;
        let cloud_node_profiles: Vec<Arc<NodeProfile>> = self.node_profile_repo.fetch_node_profiles().await?.into_iter().map(Arc::new).collect();

        let my_want_asset_keys: Vec<Arc<AssetKey>> = self.get_want_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();
        let my_push_asset_keys: HashSet<Arc<AssetKey>> = self.get_push_asset_keys_fn.call(&()).into_iter().flatten().map(Arc::new).collect();

        // 求める key は lookup の起点になる。cache 側で寿命を管理する
        self.want_cache.lock().set_local(my_want_asset_keys);

        let statuses = self.sessions.statuses().await;
        if statuses.is_empty() {
            return Ok(());
        }
        let ids: Vec<&[u8]> = statuses.iter().map(|(id, _)| id.as_slice()).collect();

        // 既知 node は全 Session へ直接送る
        let mut node_profiles = vec![my_node_profile.clone()];
        node_profiles.extend(cloud_node_profiles);
        let node_profiles = self.select_node_profiles(node_profiles, NodeMessage::MAX_ANNOUNCE_NODE_PROFILES);
        for (_, status) in statuses.iter() {
            if let Err(e) = status
                .outbox
                .send(NodeMessage::AnnounceProfiles {
                    node_profiles: node_profiles.clone(),
                })
                .await
            {
                warn!(error_message = e.to_string(), "dropped a profile announcement because the outbox is closed");
            }
        }

        // 自分が提供する key は、key に近い Session へ広告する
        let mut per_session: HashMap<Vec<u8>, HashMap<Arc<AssetKey>, AnnounceLocation>> = HashMap::new();
        for asset_key in my_push_asset_keys {
            let location = AnnounceLocation {
                hops: MAX_HOPS,
                node_profiles: vec![my_node_profile.clone()],
            };
            for id in Kadex::find(my_node_profile.id(), &asset_key.hash.value, &ids, TaskCommunicator::FAN_OUT) {
                per_session.entry(id.to_vec()).or_default().insert(asset_key.clone(), location.clone());
            }
        }

        for (id, locations) in per_session {
            let Some((_, status)) = statuses.iter().find(|(session_id, _)| session_id == &id) else {
                continue;
            };
            let locations = AnnounceLocationsBudget::select(locations);
            if locations.is_empty() {
                continue;
            }
            if let Err(e) = status.outbox.send(NodeMessage::AnnounceLocations { locations }).await {
                warn!(error_message = e.to_string(), "dropped a location announcement because the outbox is closed");
            }
        }

        Ok(())
    }

    fn select_node_profiles(&self, mut node_profiles: Vec<Arc<NodeProfile>>, max_count: usize) -> Vec<Arc<NodeProfile>> {
        node_profiles.shuffle(&mut *self.rng.lock());
        node_profiles.truncate(max_count);
        node_profiles
    }
}

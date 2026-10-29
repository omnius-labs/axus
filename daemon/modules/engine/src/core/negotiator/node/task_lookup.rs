use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use tokio::{
    select,
    sync::Mutex as TokioMutex,
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::runtime::Shutdown,
    core::negotiator::node::{LookupAnswer, LookupRegistry, NodeMessage, RoutingTable},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

/// Finds the nodes that advertise an AssetKey by asking the known nodes closest to it.
///
/// A want is a local request, so the lookup starts from it rather than from a message a peer sent.
pub struct TaskLookup {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<SessionRegistry>,
    communicator: Arc<TaskCommunicator>,
    node_profile_repo: Arc<NodeFinderRepo>,
    location_cache: Arc<Mutex<LocationCache>>,
    want_cache: Arc<Mutex<WantCache>>,
    lookup_registry: Arc<LookupRegistry>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    option: NodeFinderOption,
    attempts: Mutex<HashMap<Arc<AssetKey>, DateTime<Utc>>>,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskLookup {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }
    }
}

impl TaskLookup {
    /// The nodes one lookup asks at a time.
    pub const FAN_OUT: usize = 3;
    /// How many more times the lookup interval the same AssetKey is not looked up again.
    pub const COOLDOWN_FACTOR: u32 = 3;

    fn interval(&self) -> Duration {
        self.option.intervals.lookup
    }

    fn cooldown(&self) -> Duration {
        self.interval() * Self::COOLDOWN_FACTOR
    }

    /// How long one lookup may take in total.
    fn deadline(&self) -> Duration {
        self.option.intervals.receive_timeout()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<SessionRegistry>,
        communicator: Arc<TaskCommunicator>,
        node_profile_repo: Arc<NodeFinderRepo>,
        location_cache: Arc<Mutex<LocationCache>>,
        want_cache: Arc<Mutex<WantCache>>,
        lookup_registry: Arc<LookupRegistry>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            my_node_profile,
            sessions,
            communicator,
            node_profile_repo,
            location_cache,
            want_cache,
            lookup_registry,
            clock,
            sleeper,
            rng,
            option,
            attempts: Mutex::new(HashMap::new()),
            join_handle: Arc::new(TokioMutex::new(None)),
            token: CancellationToken::new(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            while !this.token.is_cancelled() {
                select! {
                    _ = async {
                        this.sleeper.sleep(this.interval()).await;
                        let keys = this.want_cache.lock().keys();
                        for asset_key in keys {
                            if this.token.is_cancelled() {
                                break;
                            }
                            if let Err(e) = this.lookup(&asset_key).await {
                                warn!(error_message = e.to_string(), "lookup failed");
                            }
                        }
                    } => {}
                    _ = this.token.cancelled() => break,
                }
            }
        }));

        Ok(())
    }

    /// Returns the nodes that advertise `asset_key`, asking the network when the cache has none.
    pub async fn lookup(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        let cached = self.location_cache.lock().get(asset_key).to_vec();
        if !cached.is_empty() {
            return Ok(cached);
        }

        {
            let mut attempts = self.attempts.lock();
            let now = self.clock.now();
            if attempts
                .get(asset_key)
                .is_some_and(|last| now - *last < chrono::Duration::from_std(self.cooldown()).unwrap_or_default())
            {
                return Ok(Vec::new());
            }
            attempts.insert(Arc::new(asset_key.clone()), now);
            attempts.retain(|_, last| now - *last < chrono::Duration::minutes(30));
        }

        let my_node_profile = self.my_node_profile.lock().clone();
        let deadline = Instant::now() + self.deadline();
        let known = self.node_profile_repo.fetch_node_profiles().await?;

        // 近いノードから始め、答えが返した closer_nodes で前へ進む
        let mut frontier: Vec<Arc<NodeProfile>> = RoutingTable::closest(my_node_profile.id(), &asset_key.hash.value, &known, Self::FAN_OUT, my_node_profile.id());
        let mut queried: HashSet<Vec<u8>> = HashSet::new();
        let mut found: Vec<Arc<NodeProfile>> = Vec::new();

        while !frontier.is_empty() && Instant::now() < deadline {
            let batch: Vec<Arc<NodeProfile>> = frontier.drain(..).filter(|profile| queried.insert(profile.id().to_vec())).collect();
            let mut closer: Vec<Arc<NodeProfile>> = Vec::new();

            for node_profile in batch {
                if Instant::now() >= deadline {
                    break;
                }
                let Ok(answer) = self.query(&node_profile, asset_key).await else {
                    continue;
                };
                found.extend(answer.locations);
                closer.extend(answer.closer_nodes.into_iter().filter(|profile| !queried.contains(profile.id())));
            }

            if !found.is_empty() {
                break;
            }
            frontier = closer.into_iter().take(Self::FAN_OUT).collect();
        }

        if !found.is_empty() {
            self.location_cache.lock().insert(Arc::new(asset_key.clone()), found.iter().cloned());
        }

        Ok(found)
    }

    async fn query(&self, node_profile: &Arc<NodeProfile>, asset_key: &AssetKey) -> Result<LookupAnswer> {
        let peer_id = node_profile.id().to_vec();
        let status = match self.find_session(&peer_id).await {
            Some(status) => status,
            None => self.communicator.connect(node_profile).await?,
        };

        let lookup_id = self.rng.lock().next_u64();
        let receiver = self.lookup_registry.register(&peer_id, lookup_id).await;
        if let Err(e) = status
            .outbox
            .send(NodeMessage::LookupRequest {
                lookup_id,
                asset_key: asset_key.clone(),
            })
            .await
        {
            self.lookup_registry.remove(&peer_id, lookup_id).await;
            return Err(Error::from_error(e, ErrorKind::NetworkError).with_message("dropped a lookup request"));
        }

        match timeout(self.option.intervals.communicate, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(e)) => Err(Error::from_error(e, ErrorKind::NetworkError).with_message("the lookup answer was dropped")),
            Err(e) => {
                self.lookup_registry.remove(&peer_id, lookup_id).await;
                Err(Error::from_error(e, ErrorKind::NetworkError).with_message("the lookup answer timed out"))
            }
        }
    }

    async fn find_session(&self, peer_id: &[u8]) -> Option<Arc<SessionStatus>> {
        self.sessions
            .statuses()
            .await
            .into_iter()
            .find(|(id, _)| id.as_slice() == peer_id)
            .map(|(_, status)| status)
    }
}

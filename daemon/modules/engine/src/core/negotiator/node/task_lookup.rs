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
    time::{Instant, timeout, timeout_at},
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

        // 近いノードから始め、答えが返した closer_nodes のうち今より近いものだけを次に問い合わせる
        let mut frontier: Vec<Arc<NodeProfile>> = RoutingTable::closest(my_node_profile.id(), &asset_key.hash.value, &known, Self::FAN_OUT, my_node_profile.id());
        let mut queried: HashSet<Vec<u8>> = HashSet::new();
        let mut found: Vec<Arc<NodeProfile>> = Vec::new();
        let mut closest: Option<Vec<u8>> = None;

        while !frontier.is_empty() && Instant::now() < deadline {
            let batch: Vec<Arc<NodeProfile>> = frontier.drain(..).filter(|profile| queried.insert(profile.id().to_vec())).collect();
            let mut closer: Vec<Arc<NodeProfile>> = Vec::new();

            for node_profile in batch {
                if Instant::now() >= deadline {
                    break;
                }
                // 近さは Kadex の順序で比べる。バイト列の辞書順とは一致しない
                if closest
                    .as_ref()
                    .is_none_or(|value| RoutingTable::is_closer(&asset_key.hash.value, node_profile.id(), value))
                {
                    closest = Some(node_profile.id().to_vec());
                }
                let Ok(answer) = self.query(&node_profile, asset_key, deadline).await else {
                    continue;
                };
                found.extend(answer.locations);
                closer.extend(answer.closer_nodes.into_iter().filter(|profile| {
                    let id = profile.id();
                    id != my_node_profile.id() && !queried.contains(id) && closest.as_ref().is_none_or(|value| RoutingTable::is_closer(&asset_key.hash.value, id, value))
                }));
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

    async fn query(&self, node_profile: &Arc<NodeProfile>, asset_key: &AssetKey, deadline: Instant) -> Result<LookupAnswer> {
        let peer_id = node_profile.id().to_vec();
        let status = match self.find_session(&peer_id).await {
            Some(status) => status,
            // ダイヤルも期限の内側に収める
            None => timeout_at(deadline, self.communicator.connect(node_profile))
                .await
                .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("the lookup dial timed out"))??,
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

        // 応答は 1 周期より遅れることがあるので受信期限まで待ち、全体の期限でも切る
        let wait = deadline.saturating_duration_since(Instant::now()).min(self.option.intervals.receive_timeout());
        match timeout(wait, receiver).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(e)) => {
                self.lookup_registry.remove(&peer_id, lookup_id).await;
                Err(Error::from_error(e, ErrorKind::NetworkError).with_message("the lookup answer was dropped"))
            }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::DateTime;
    use rand::SeedableRng as _;
    use testresult::TestResult;
    use tokio::net::TcpStream;

    use omnius_core_base::{clock::FakeClockUtc, sleeper::SleeperImpl};
    use omnius_core_omnikit::{
        generated::omni_sign::{OmniSignType, OmniSigner},
        model::omni_addr::OmniAddr,
    };

    use crate::{
        base::connection::ConnectionTcpConnector,
        core::session::{SessionConnector, model::SessionOption},
    };

    use super::*;

    fn clock() -> Arc<FakeClockUtc> {
        Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into()))
    }

    fn option(communicate: Duration) -> NodeFinderOption {
        NodeFinderOption {
            state_dir: String::new(),
            max_connected_session_count: 8,
            max_accepted_session_count: 8,
            intervals: NodeFinderIntervals {
                connect: Duration::from_millis(10),
                compute: Duration::from_millis(10),
                communicate,
                lookup: Duration::from_millis(10),
            },
        }
    }

    async fn create_lookup(
        clock: Arc<FakeClockUtc>,
        option: NodeFinderOption,
        tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>,
    ) -> Result<(Arc<TaskLookup>, Arc<TaskCommunicator>, Arc<NodeFinderRepo>)> {
        let dir = tempfile::tempdir()?;
        let state_dir = dir.path().to_str().unwrap().to_string();
        std::mem::forget(dir);

        let my_node_profile = Arc::new(Mutex::new(NodeProfile::random_for_test(vec![OmniAddr::new("self")])));
        let sessions = Arc::new(SessionRegistry::new());
        let node_profile_repo = Arc::new(NodeFinderRepo::new(&state_dir, clock.clone()).await?);
        let location_cache = Arc::new(Mutex::new(LocationCache::new(clock.clone())));
        let want_cache = Arc::new(Mutex::new(WantCache::new()));
        let lookup_registry = Arc::new(LookupRegistry::new());
        let rng: Arc<Mutex<dyn rand::Rng + Send + Sync>> = Arc::new(Mutex::new(rand::rngs::ChaCha20Rng::from_rng(&mut rand_core::UnwrapErr(rand::rngs::SysRng))));

        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let session_connector = Arc::new(SessionConnector::new(tcp_connector, signer, clock.clone(), rng.clone(), SessionOption::default()));

        let (session_sender, session_receiver) = mpsc::channel(20);
        std::mem::forget(session_sender);

        let communicator = TaskCommunicator::new(
            my_node_profile.clone(),
            sessions.clone(),
            session_connector,
            node_profile_repo.clone(),
            location_cache.clone(),
            lookup_registry.clone(),
            Arc::new(TokioMutex::new(session_receiver)),
            clock.clone(),
            Arc::new(SleeperImpl),
            option.clone(),
        )
        .await?;

        let lookup = TaskLookup::new(
            my_node_profile,
            sessions,
            communicator.clone(),
            node_profile_repo.clone(),
            location_cache,
            want_cache,
            lookup_registry,
            clock,
            Arc::new(SleeperImpl),
            rng,
            option,
        )
        .await?;

        Ok((lookup, communicator, node_profile_repo))
    }

    fn asset_key() -> AssetKey {
        AssetKey {
            typ: "test".to_string(),
            hash: omnius_core_omnikit::generated::omni_hash::OmniHash {
                typ: omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType::Sha3_256,
                value: vec![1; 32],
            },
        }
    }

    #[derive(Default)]
    struct CountingConnector {
        count: AtomicUsize,
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl ConnectionTcpConnector for CountingConnector {
        async fn connect(&self, _addr: &OmniAddr) -> Result<TcpStream> {
            self.count.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            Err(Error::new(ErrorKind::NotConnected).with_message("the test does not dial"))
        }
    }

    #[tokio::test]
    async fn the_same_key_is_not_dialed_again_inside_the_cooldown() -> TestResult {
        let clock = clock();
        let connector = Arc::new(CountingConnector::default());
        let (lookup, communicator, repo) = create_lookup(clock.clone(), option(Duration::from_millis(100)), connector.clone()).await?;

        // 既知 node を 1 つ用意する。接続はできずダイヤルだけが数えられる
        let known = NodeProfile::random_for_test(vec![OmniAddr::new("known")]);
        repo.insert_or_update_node_profiles(&[&known]).await?;

        let first = lookup.lookup(&asset_key()).await?;
        assert!(first.is_empty());
        assert_eq!(connector.count.load(Ordering::SeqCst), 1);

        // cooldown の内側では、同じ key を問い合わせ直さない
        let second = lookup.lookup(&asset_key()).await?;
        assert!(second.is_empty());
        assert_eq!(connector.count.load(Ordering::SeqCst), 1, "a lookup inside the cooldown does not dial");

        clock.advance(Duration::from_millis(100) * TaskLookup::COOLDOWN_FACTOR + Duration::from_millis(1));
        let third = lookup.lookup(&asset_key()).await?;
        assert!(third.is_empty());
        assert_eq!(connector.count.load(Ordering::SeqCst), 2, "the cooldown ends and the key is dialed again");

        lookup.shutdown().await;
        communicator.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_lookup_ends_within_its_deadline_even_when_dialing_hangs() -> TestResult {
        let clock = clock();
        let connector = Arc::new(CountingConnector {
            count: AtomicUsize::new(0),
            delay: Duration::from_secs(30),
        });
        let option = option(Duration::from_millis(100));
        let (lookup, communicator, repo) = create_lookup(clock, option.clone(), connector.clone()).await?;

        let known = NodeProfile::random_for_test(vec![OmniAddr::new("known")]);
        repo.insert_or_update_node_profiles(&[&known]).await?;

        let started = Instant::now();
        let found = lookup.lookup(&asset_key()).await?;
        let elapsed = started.elapsed();

        assert!(found.is_empty());
        assert!(
            elapsed < option.intervals.receive_timeout() + Duration::from_millis(500),
            "the lookup is cut at its deadline, elapsed {elapsed:?}"
        );
        assert_eq!(connector.count.load(Ordering::SeqCst), 1);

        lookup.shutdown().await;
        communicator.shutdown().await;
        Ok(())
    }
}

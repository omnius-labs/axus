use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};

use crate::{
    base::runtime::Shutdown,
    core::session::{SessionAccepter, SessionConnector},
    prelude::*,
};

use super::*;

#[derive(Debug, Clone)]
pub struct FileExchangerOption {
    pub state_dir: PathBuf,
    /// Sessions dialed for the root hashes of Downloading subscriptions.
    pub max_connected_session_for_fetch_count: usize,
    /// Sessions dialed for the other provided root hashes.
    pub max_connected_session_for_serve_count: usize,
    pub max_accepted_session_count: usize,
    pub max_session_per_root_hash_count: usize,
    /// The nodes unchoked in the order of received bytes. One more node is unchoked at random.
    pub unchoke_count: usize,
}

impl FileExchangerOption {
    pub fn new(state_dir: PathBuf) -> Self {
        Self {
            state_dir,
            max_connected_session_for_fetch_count: 16,
            max_connected_session_for_serve_count: 8,
            max_accepted_session_count: 16,
            max_session_per_root_hash_count: 8,
            unchoke_count: 4,
        }
    }
}

/// Carries the blocks of the published and subscribed files between nodes.
pub struct FileExchanger {
    ctx: Arc<ExchangeContext>,
    task_connectors: Vec<Arc<TaskConnector>>,
    task_accepter: Arc<TaskAccepter>,
    task_upkeep: Arc<TaskUpkeep>,
}

#[async_trait]
impl Shutdown for FileExchanger {
    async fn shutdown(&self) {
        // Cancelling first lets every task and Session worker end without waiting for a deadline.
        self.ctx.cancellation_token.cancel();
        for task in &self.task_connectors {
            task.shutdown().await;
        }
        self.task_accepter.shutdown().await;
        self.task_upkeep.shutdown().await;
        self.ctx.join_workers().await;
        self.ctx.file_publisher.shutdown().await;
        self.ctx.file_subscriber.shutdown().await;
    }
}

impl FileExchanger {
    const CONNECTOR_COUNT: usize = 3;

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        location_finder: Arc<dyn LocationFinder>,
        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: FileExchangerOption,
        intervals: FileExchangerIntervals,
    ) -> Result<Self> {
        let file_publisher = FilePublisher::new(&option.state_dir.join("file_publisher"), tsid_provider.clone(), clock.clone(), sleeper.clone()).await?;
        let file_subscriber = match FileSubscriber::new(&option.state_dir.join("file_subscriber"), tsid_provider, clock, sleeper.clone()).await {
            Ok(file_subscriber) => file_subscriber,
            Err(error) => {
                file_publisher.shutdown().await;
                return Err(error);
            }
        };

        let ctx = Arc::new(ExchangeContext::new(file_publisher, file_subscriber, location_finder, rng.clone(), option, intervals));
        // The first refresh makes the provided root hashes known before any task decides on them.
        if let Err(error) = ctx.refresh_root_hashes().await {
            ctx.file_publisher.shutdown().await;
            ctx.file_subscriber.shutdown().await;
            return Err(error);
        }

        let task_connectors = (0..Self::CONNECTOR_COUNT)
            .map(|_| TaskConnector::new(ctx.clone(), session_connector.clone(), sleeper.clone(), rng.clone()))
            .collect();
        let task_accepter = TaskAccepter::new(ctx.clone(), session_accepter);
        let task_upkeep = TaskUpkeep::new(ctx.clone(), sleeper);

        Ok(Self {
            ctx,
            task_connectors,
            task_accepter,
            task_upkeep,
        })
    }

    #[cfg(test)]
    pub fn context(&self) -> &Arc<ExchangeContext> {
        &self.ctx
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, path::Path, sync::Arc, time::Duration};

    use chrono::Utc;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;
    use tokio::time::Instant;
    use tokio_util::sync::CancellationToken;

    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::{Sleeper, SleeperImpl},
        tsid::{TsidProvider, TsidProviderImpl},
    };
    use omnius_core_omnikit::{
        generated::{
            omni_hash::{OmniHash, OmniHashAlgorithmType},
            omni_sign::{OmniSignType, OmniSigner},
        },
        model::omni_addr::OmniAddr,
    };

    use crate::{
        base::{
            connection::{ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, TcpProxyOption, TcpProxyType},
            runtime::Shutdown as _,
        },
        core::{
            identity::NodeIdentity,
            negotiator::{NodeFinder, NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, NodeProfileFetcherImpl},
            session::{
                SessionAccepter, SessionConnector,
                model::{Session, SessionOption, SessionType},
            },
        },
        generated::axus::file::{ExchangeMessage, MAX_PENDING_REQUESTS},
        model::NodeProfile,
        prelude::*,
        protocol::file::ExchangeMessageCodec,
    };

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_secs(30);
    const BLOCK_SIZE: u32 = 256;

    fn fast_intervals() -> FileExchangerIntervals {
        FileExchangerIntervals {
            status_cycle: Duration::from_millis(200),
            request_timeout: Duration::from_secs(1),
            // Retraction stays off, so a test that does not arrange for it never sees a request retracted.
            request_retract_after: Duration::from_secs(60),
            unwanted_close: Duration::from_millis(800),
            unwanted_cooldown: Duration::from_secs(2),
            reject_retry: Duration::from_millis(300),
            rank_absent_retry: Duration::from_millis(200),
            cancel_grace: Duration::from_secs(2),
            // The tail stays off, so a test that does not arrange for it transfers one copy of each block.
            tail_remaining: 0,
            tail_stall_remaining: 0,
            tail_stall: Duration::from_millis(500),
            tail_copies: 2,
            fetch_window: Duration::from_secs(10),
            fetch_min_blocks: 4,
            pending_min: 4,
            ranking_window: Duration::from_secs(10),
            unchoke_rotation: Duration::from_millis(300),
            violation_exclusion: Duration::from_secs(60),
            timeout_exclusion: Duration::from_secs(2),
            poll: Duration::from_millis(20),
            connect: Duration::from_millis(50),
            wanted_refresh: Duration::from_millis(20),
        }
    }

    async fn wait_for<F, Fut>(mut condition: F) -> TestResult
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        let deadline = Instant::now() + TEST_TIMEOUT;
        while !condition().await {
            assert!(Instant::now() < deadline, "the condition did not hold within the timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    fn content(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// What a node needs to be reached: the key it proves in the Session and the address it listens on.
    struct Endpoint {
        session_accepter: Arc<SessionAccepter>,
        session_connector: Arc<SessionConnector>,
        profile: NodeProfile,
    }

    impl Endpoint {
        async fn new(dir: &Path, supported_types: &[SessionType]) -> TestResult<Self> {
            let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
            let sleeper: Arc<dyn Sleeper + Send + Sync> = Arc::new(SleeperImpl);
            let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
            let identity = NodeIdentity::load_or_create(&dir.join("identity")).await?;
            let signer = identity.signer();

            let tcp_accepter = ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 0), false).await?;
            let addrs = tcp_accepter.get_advertised_addrs().await?;
            let tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync> = Arc::new(tcp_accepter);
            let tcp_connector = Arc::new(
                ConnectionTcpConnectorImpl::new(TcpProxyOption {
                    typ: TcpProxyType::None,
                    addr: None,
                })
                .await?,
            );
            let session_accepter =
                Arc::new(SessionAccepter::new(tcp_accepter, signer.clone(), clock.clone(), sleeper, rng.clone(), supported_types, SessionOption::default()).await);
            let session_connector = Arc::new(SessionConnector::new(tcp_connector, signer.clone(), clock.clone(), rng, SessionOption::default()));
            let profile = NodeProfile::sign(&signer, addrs, clock.now())?;

            Ok(Self {
                session_accepter,
                session_connector,
                profile,
            })
        }
    }

    /// A FileExchanger node. Its peers are told by the test, or found through a real NodeFinder.
    struct TestNode {
        exchanger: FileExchanger,
        endpoint: Endpoint,
        locations: Option<Arc<FixedLocationFinder>>,
        node_finder: Option<Arc<NodeFinder>>,
        dir: tempfile::TempDir,
    }

    impl TestNode {
        async fn new() -> TestResult<Self> {
            Self::with_option(|_| {}).await
        }

        async fn with_option(adjust: impl FnOnce(&mut FileExchangerOption)) -> TestResult<Self> {
            Self::with_intervals(adjust, fast_intervals()).await
        }

        async fn with_intervals(adjust: impl FnOnce(&mut FileExchangerOption), intervals: FileExchangerIntervals) -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let endpoint = Endpoint::new(dir.path(), &[SessionType::FileExchanger]).await?;
            let locations = Arc::new(FixedLocationFinder::new(endpoint.profile.id().to_vec()));
            let mut option = FileExchangerOption::new(dir.path().join("exchanger"));
            adjust(&mut option);
            let exchanger = FileExchanger::new(
                endpoint.session_connector.clone(),
                endpoint.session_accepter.clone(),
                locations.clone(),
                tsid_provider(),
                Arc::new(ClockUtc),
                Arc::new(SleeperImpl),
                Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)))),
                option,
                intervals,
            )
            .await?;

            Ok(Self {
                exchanger,
                endpoint,
                locations: Some(locations),
                node_finder: None,
                dir,
            })
        }

        /// A node that finds its peers through NodeFinder, starting from the bootstrap nodes.
        async fn with_node_finder(bootstrap: &[&NodeProfile]) -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let endpoint = Endpoint::new(dir.path(), &[SessionType::NodeFinder, SessionType::FileExchanger]).await?;
            let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
            let sleeper: Arc<dyn Sleeper + Send + Sync> = Arc::new(SleeperImpl);
            let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));

            let repo_dir = dir.path().join("repo");
            tokio::fs::create_dir_all(&repo_dir).await?;
            let node_profile_repo = Arc::new(NodeFinderRepo::new(repo_dir.to_str().unwrap(), clock.clone()).await?);
            node_profile_repo.insert_or_update_node_profiles(bootstrap).await?;
            let finder_dir = dir.path().join("finder");
            tokio::fs::create_dir_all(&finder_dir).await?;
            let node_finder = Arc::new(
                NodeFinder::new(
                    endpoint.profile.clone(),
                    endpoint.session_connector.clone(),
                    endpoint.session_accepter.clone(),
                    node_profile_repo,
                    Arc::new(NodeProfileFetcherImpl::new(&[])),
                    clock.clone(),
                    sleeper.clone(),
                    rng.clone(),
                    NodeFinderOption {
                        state_dir: finder_dir.to_str().unwrap().to_string(),
                        max_connected_session_count: 3,
                        max_accepted_session_count: 3,
                        intervals: NodeFinderIntervals {
                            connect: Duration::from_millis(100),
                            compute: Duration::from_millis(100),
                            communicate: Duration::from_millis(500),
                            lookup: Duration::from_millis(100),
                        },
                    },
                )
                .await?,
            );
            let exchanger = FileExchanger::new(
                endpoint.session_connector.clone(),
                endpoint.session_accepter.clone(),
                Arc::new(NodeFinderLocationFinder::new(node_finder.clone())),
                tsid_provider(),
                clock,
                sleeper,
                rng,
                FileExchangerOption::new(dir.path().join("exchanger")),
                fast_intervals(),
            )
            .await?;

            Ok(Self {
                exchanger,
                endpoint,
                locations: None,
                node_finder: Some(node_finder),
                dir,
            })
        }

        fn id(&self) -> Vec<u8> {
            self.endpoint.profile.id().to_vec()
        }

        fn ctx(&self) -> &Arc<ExchangeContext> {
            self.exchanger.context()
        }

        fn knows(&self, peers: &[&NodeProfile]) {
            self.locations
                .as_ref()
                .expect("a node with fixed locations")
                .set_locations(peers.iter().map(|peer| (*peer).clone()).collect());
        }

        async fn publish(&self, content: &[u8]) -> TestResult<OmniHash> {
            let source = self.dir.path().join("source.bin");
            tokio::fs::write(&source, content).await?;
            let publisher = &self.ctx().file_publisher;
            publisher.import(source.to_str().unwrap(), "source.bin", BLOCK_SIZE, None, 0).await?;
            wait_for(|| async { !publisher.get_published_root_hashes().await.unwrap().is_empty() }).await?;
            Ok(publisher.get_published_root_hashes().await?.pop().unwrap())
        }

        async fn subscribe(&self, root_hash: &OmniHash, name: &str) -> TestResult<(String, std::path::PathBuf)> {
            let output = self.dir.path().join(name);
            let id = self.ctx().file_subscriber.subscribe(root_hash, output.to_str().unwrap(), None, 0).await?;
            Ok((id, output))
        }

        async fn wait_completed(&self, id: &str) -> TestResult {
            let subscriber = self.ctx().file_subscriber.clone();
            wait_for(|| {
                let subscriber = subscriber.clone();
                let id = id.to_string();
                async move {
                    let file = subscriber.find_file(&id).await.unwrap().unwrap();
                    assert!(
                        !matches!(file.status, SubscribedFileStatus::Failed | SubscribedFileStatus::Canceled),
                        "the subscription ended without completing: {:?}",
                        file.failed_reason
                    );
                    file.status == SubscribedFileStatus::Completed
                }
            })
            .await
        }

        async fn shutdown(&self) {
            self.exchanger.shutdown().await;
            if let Some(node_finder) = &self.node_finder {
                node_finder.shutdown().await;
            }
            self.endpoint.session_accepter.shutdown().await;
        }
    }

    /// A peer that speaks the handshake of FileExchanger by hand, so that a test controls every message after it.
    struct RawPeer {
        endpoint: Endpoint,
        /// The requests received, so that a test sees how often the peer was asked.
        requests: Arc<std::sync::atomic::AtomicUsize>,
        _dir: tempfile::TempDir,
    }

    #[derive(Clone, Copy)]
    enum Behavior {
        /// Answers a request with a block whose content does not match its hash.
        BadBlock,
        /// Sends a block nobody requested.
        UnrequestedBlock,
        /// Answers every request with a presence bitmap that has no set bit.
        NoBlocks,
        /// Answers a request with a presence bitmap whose number of bits does not match the request.
        ShortHaveMap,
        /// Unchokes the requester and keeps the Session alive, but never answers.
        IgnoreRequests,
        /// Finishes the handshake and then says nothing.
        Silent,
    }

    impl RawPeer {
        async fn new() -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            Ok(Self {
                endpoint: Endpoint::new(dir.path(), &[SessionType::FileExchanger]).await?,
                requests: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                _dir: dir,
            })
        }

        fn id(&self) -> Vec<u8> {
            self.endpoint.profile.id().to_vec()
        }

        fn request_count(&self) -> usize {
            self.requests.load(std::sync::atomic::Ordering::SeqCst)
        }

        async fn dial(&self, target: &NodeProfile, root_hash: &OmniHash) -> TestResult<Session> {
            let session = self.endpoint.session_connector.connect(&target.addrs()[0], &SessionType::FileExchanger).await?;
            ExchangeHandshake::dial(&session, root_hash, TEST_TIMEOUT).await?;
            Ok(session)
        }

        /// Accepts Sessions for any root hash and plays the behavior on each, until the token is cancelled.
        fn serve(self: &Arc<Self>, behavior: Behavior, token: CancellationToken) -> tokio::task::JoinHandle<()> {
            let this = self.clone();
            tokio::spawn(async move {
                loop {
                    let session = tokio::select! {
                        session = this.endpoint.session_accepter.accept(&SessionType::FileExchanger) => session,
                        _ = token.cancelled() => return,
                    };
                    let Ok(session) = session else { return };
                    if ExchangeHandshake::accept(&session, TEST_TIMEOUT, |_| true).await.is_err() {
                        continue;
                    }
                    let (token, requests) = (token.clone(), this.requests.clone());
                    tokio::spawn(async move {
                        let _ = Self::play(session, behavior, token, requests).await;
                    });
                }
            })
        }

        async fn play(session: Session, behavior: Behavior, token: CancellationToken, requests: Arc<std::sync::atomic::AtomicUsize>) -> Result<()> {
            if matches!(behavior, Behavior::Silent) {
                token.cancelled().await;
                return Ok(());
            }
            let status = ExchangeMessage::Status { want: false, choke: false };
            Self::send(&session, &status).await?;
            if matches!(behavior, Behavior::UnrequestedBlock) {
                Self::send(
                    &session,
                    &ExchangeMessage::Block {
                        block_hash: OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"nobody asked"),
                        value: b"nobody asked".to_vec(),
                    },
                )
                .await?;
            }
            let mut tick = tokio::time::interval(Duration::from_millis(50));
            loop {
                tokio::select! {
                    _ = token.cancelled() => return Ok(()),
                    _ = tick.tick() => Self::send(&session, &status).await?,
                    message = Self::receive(&session) => {
                        let message = message?;
                        if matches!(message, ExchangeMessage::Request { .. }) {
                            requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        match (behavior, message) {
                            (Behavior::BadBlock, ExchangeMessage::Request { block_hashes }) => {
                                Self::send(&session, &ExchangeMessage::Block { block_hash: block_hashes[0].clone(), value: vec![0; 10] }).await?;
                            }
                            (Behavior::NoBlocks, ExchangeMessage::Request { block_hashes }) => {
                                let bits = vec![0u8; block_hashes.len().div_ceil(8)];
                                Self::send(&session, &ExchangeMessage::HaveMap { bits }).await?;
                            }
                            (Behavior::ShortHaveMap, ExchangeMessage::Request { .. }) => {
                                Self::send(&session, &ExchangeMessage::HaveMap { bits: vec![0b0000_0001] }).await?;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        async fn send(session: &Session, message: &ExchangeMessage) -> Result<()> {
            session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(message).await
        }

        async fn receive(session: &Session) -> Result<ExchangeMessage> {
            session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await
        }
    }

    fn tsid_provider() -> Arc<Mutex<dyn TsidProvider + Send + Sync>> {
        Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)), 8)))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_multi_rank_file_is_transferred_between_two_nodes() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        publisher.knows(&[&subscriber.endpoint.profile]);
        subscriber.knows(&[&publisher.endpoint.profile]);
        let content = content(64 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.wait_completed(&id).await?;

        assert_eq!(tokio::fs::read(&output).await?, content);
        publisher.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_slow_node_keeps_fewer_requests_awaiting_responses() -> TestResult {
        let node = TestNode::new().await?;
        let ctx = node.ctx();
        let (fast, slow) = (b"fast".to_vec(), b"slow".to_vec());
        for _ in 0..4 {
            ctx.record_received(&fast, 1_000_000);
            ctx.record_received(&slow, 1_000);
        }

        // The fast node is at or above the median, the slow one below it, and a node with no record keeps the maximum.
        assert_eq!(ctx.pending_limit(&fast), MAX_PENDING_REQUESTS as usize);
        assert_eq!(ctx.pending_limit(&slow), ctx.intervals.pending_min);
        assert_eq!(ctx.pending_limit(b"unknown"), MAX_PENDING_REQUESTS as usize);

        node.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_node_that_provided_fastest_is_dialed_first() -> TestResult {
        let fast = TestNode::new().await?;
        let slow = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let content = content(8 * 1024);
        let root_hash = fast.publish(&content).await?;
        assert_eq!(slow.publish(&content).await?, root_hash);
        subscriber.knows(&[&fast.endpoint.profile, &slow.endpoint.profile]);

        let ctx = subscriber.ctx();
        for _ in 0..4 {
            ctx.record_received(&fast.id(), 1_000_000);
            ctx.record_received(&slow.id(), 1_000);
        }

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        let (ctx, fast_id, slow_id, watched_root) = (subscriber.ctx().clone(), fast.id(), slow.id(), root_hash.clone());
        wait_for(|| {
            let (ctx, fast_id, slow_id, watched_root) = (ctx.clone(), fast_id.clone(), slow_id.clone(), watched_root.clone());
            async move { ctx.registry.contains(&fast_id, &watched_root) && !ctx.registry.contains(&slow_id, &watched_root) }
        })
        .await?;
        subscriber.wait_completed(&id).await?;

        assert_eq!(tokio::fs::read(&output).await?, content);
        fast.shutdown().await;
        slow.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_block_in_the_tail_is_assigned_to_two_sessions_and_the_first_copy_completes_it() -> TestResult {
        // Two nodes hold the same file, so the subscriber has two Sessions for the root hash.
        let first_publisher = TestNode::new().await?;
        let second_publisher = TestNode::new().await?;
        let intervals = FileExchangerIntervals {
            // Every block counts as the tail, so both Sessions are asked for the same block.
            tail_remaining: 4096,
            // The duplicate copies double the traffic, so a loaded machine needs more than the usual deadline.
            request_timeout: Duration::from_secs(10),
            ..fast_intervals()
        };
        let subscriber = TestNode::with_intervals(|_| {}, intervals).await?;
        let content = content(8 * 1024);
        let root_hash = first_publisher.publish(&content).await?;
        let same_root_hash = second_publisher.publish(&content).await?;
        assert_eq!(same_root_hash, root_hash);
        subscriber.knows(&[&first_publisher.endpoint.profile, &second_publisher.endpoint.profile]);

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        wait_for(|| {
            let ctx = subscriber.ctx().clone();
            let root_hash = root_hash.clone();
            async move { ctx.registry.count_by_root_hash(&root_hash) >= 2 }
        })
        .await?;
        subscriber.wait_completed(&id).await?;

        assert_eq!(tokio::fs::read(&output).await?, content);
        // A copy that arrives after the cancel is dropped, so neither provider is excluded for sending it.
        assert!(!subscriber.ctx().is_excluded(&first_publisher.id()));
        assert!(!subscriber.ctx().is_excluded(&second_publisher.id()));

        first_publisher.shutdown().await;
        second_publisher.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_node_in_the_middle_of_fetching_hands_blocks_to_a_third_node() -> TestResult {
        let publisher = TestNode::new().await?;
        let relay = TestNode::new().await?;
        let third = TestNode::new().await?;
        // The third node knows only the relay, so every block it gets comes from a node that was fetching them itself.
        publisher.knows(&[]);
        relay.knows(&[&publisher.endpoint.profile, &third.endpoint.profile]);
        third.knows(&[&relay.endpoint.profile]);
        let content = content(64 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (relay_id, relay_output) = relay.subscribe(&root_hash, "relay.bin").await?;
        let (third_id, third_output) = third.subscribe(&root_hash, "third.bin").await?;
        relay.wait_completed(&relay_id).await?;
        third.wait_completed(&third_id).await?;

        assert_eq!(tokio::fs::read(&relay_output).await?, content);
        assert_eq!(tokio::fs::read(&third_output).await?, content);
        assert!(!publisher.ctx().registry.contains(&third.id(), &root_hash));
        for node in [&publisher, &relay, &third] {
            node.shutdown().await;
        }
        Ok(())
    }

    async fn expect_exclusion_then_transfer(behavior: Behavior, expect_transfer: bool) -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let peer = Arc::new(RawPeer::new().await?);
        let token = CancellationToken::new();
        let serving = peer.serve(behavior, token.clone());
        let content = content(16 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.knows(&[&peer.endpoint.profile]);
        let ctx = subscriber.ctx().clone();
        let peer_id = peer.id();
        wait_for(|| {
            let (ctx, peer_id) = (ctx.clone(), peer_id.clone());
            async move { ctx.is_excluded(&peer_id) }
        })
        .await?;
        assert_eq!(subscriber.ctx().registry.count_by_root_hash(&root_hash), 0);

        if expect_transfer {
            // Another node still provides the file, and the subscription completes without the excluded one.
            subscriber.knows(&[&peer.endpoint.profile, &publisher.endpoint.profile]);
            subscriber.wait_completed(&id).await?;
            assert_eq!(tokio::fs::read(&output).await?, content);
        }

        token.cancel();
        let _ = serving.await;
        publisher.shutdown().await;
        subscriber.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_sending_a_block_that_does_not_match_its_hash_is_excluded_and_the_file_completes_from_another_peer() -> TestResult {
        expect_exclusion_then_transfer(Behavior::BadBlock, true).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_sending_a_block_nobody_requested_is_excluded() -> TestResult {
        expect_exclusion_then_transfer(Behavior::UnrequestedBlock, true).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_request_that_is_not_answered_is_retracted_without_closing_the_session() -> TestResult {
        let publisher = TestNode::new().await?;
        let peer = Arc::new(RawPeer::new().await?);
        let token = CancellationToken::new();
        let serving = peer.serve(Behavior::IgnoreRequests, token.clone());
        let intervals = FileExchangerIntervals {
            // The request is retracted long before the Session would be closed for silence.
            request_retract_after: Duration::from_millis(100),
            request_timeout: Duration::from_secs(30),
            ..fast_intervals()
        };
        let subscriber = TestNode::with_intervals(|_| {}, intervals).await?;
        let content = content(8 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (id, _output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.knows(&[&peer.endpoint.profile]);
        let requests = peer.requests.clone();
        wait_for(|| {
            let requests = requests.clone();
            async move { requests.load(std::sync::atomic::Ordering::SeqCst) > 0 }
        })
        .await?;
        let first = peer.request_count();

        // The unanswered requests are retracted, and their blocks come back to be asked again.
        wait_for(|| {
            let requests = requests.clone();
            async move { requests.load(std::sync::atomic::Ordering::SeqCst) > 1 }
        })
        .await?;
        assert!(peer.request_count() > first);
        // The peer keeps its Session and is not excluded for not answering, which the request deadline decides.
        assert!(!subscriber.ctx().is_excluded(&peer.id()));
        assert_eq!(subscriber.ctx().registry.count_by_root_hash(&root_hash), 1);
        assert!(matches!(
            subscriber.ctx().file_subscriber.find_file(&id).await?.map(|file| file.status),
            Some(SubscribedFileStatus::Downloading)
        ));
        let _ = id;

        token.cancel();
        let _ = serving.await;
        publisher.shutdown().await;
        subscriber.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_presence_bitmap_that_does_not_match_the_request_is_a_violation() -> TestResult {
        expect_exclusion_then_transfer(Behavior::ShortHaveMap, true).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_that_holds_nothing_is_not_asked_again_for_the_rank_and_the_file_completes_from_another_peer() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let peer = Arc::new(RawPeer::new().await?);
        let token = CancellationToken::new();
        let serving = peer.serve(Behavior::NoBlocks, token.clone());
        let content = content(16 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.knows(&[&peer.endpoint.profile]);
        let requests = peer.requests.clone();
        wait_for(|| {
            let requests = requests.clone();
            async move { requests.load(std::sync::atomic::Ordering::SeqCst) > 0 }
        })
        .await?;

        // The answer with no set bit releases the request and stops asking the peer for that rank for a while.
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(peer.request_count(), 1);
        // Holding nothing is not a violation.
        assert!(!subscriber.ctx().is_excluded(&peer.id()));

        subscriber.knows(&[&peer.endpoint.profile, &publisher.endpoint.profile]);
        subscriber.wait_completed(&id).await?;
        assert_eq!(tokio::fs::read(&output).await?, content);

        token.cancel();
        let _ = serving.await;
        publisher.shutdown().await;
        subscriber.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_that_does_not_answer_requests_is_closed_at_the_request_deadline() -> TestResult {
        expect_exclusion_then_transfer(Behavior::IgnoreRequests, false).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_peer_that_goes_silent_is_closed_at_the_receive_deadline() -> TestResult {
        expect_exclusion_then_transfer(Behavior::Silent, false).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_bad_block_is_never_stored() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let peer = Arc::new(RawPeer::new().await?);
        let token = CancellationToken::new();
        let serving = peer.serve(Behavior::BadBlock, token.clone());
        let root_hash = publisher.publish(&content(1024)).await?;

        let (id, _) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.knows(&[&peer.endpoint.profile]);
        let ctx = subscriber.ctx().clone();
        let peer_id = peer.id();
        wait_for(|| {
            let (ctx, peer_id) = (ctx.clone(), peer_id.clone());
            async move { ctx.is_excluded(&peer_id) }
        })
        .await?;

        let file = subscriber.ctx().file_subscriber.find_file(&id).await?.unwrap();
        assert_eq!(file.block_count_downloaded, 0);
        assert_eq!(subscriber.ctx().file_subscriber.read_provided_block(&root_hash, &root_hash).await?, None);

        token.cancel();
        let _ = serving.await;
        publisher.shutdown().await;
        subscriber.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_requester_exceeding_the_requests_awaiting_responses_is_excluded() -> TestResult {
        let publisher = TestNode::new().await?;
        let peer = RawPeer::new().await?;
        let root_hash = publisher.publish(&content(4 * 1024)).await?;

        let session = peer.dial(&publisher.endpoint.profile, &root_hash).await?;
        RawPeer::send(&session, &ExchangeMessage::Status { want: true, choke: true }).await?;
        loop {
            if let ExchangeMessage::Status { choke: false, .. } = RawPeer::receive(&session).await? {
                break;
            }
        }
        let unknown: Vec<OmniHash> = (0..64u8).map(|i| OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [i])).collect();
        for _ in 0..200 {
            if RawPeer::send(&session, &ExchangeMessage::Request { block_hashes: unknown.clone() }).await.is_err() {
                break;
            }
        }

        let ctx = publisher.ctx().clone();
        let peer_id = peer.id();
        wait_for(|| {
            let (ctx, peer_id) = (ctx.clone(), peer_id.clone());
            async move { ctx.is_excluded(&peer_id) }
        })
        .await?;
        publisher.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_request_that_crosses_the_choke_is_ignored_and_a_request_after_the_unchoke_is_served() -> TestResult {
        let publisher = TestNode::new().await?;
        let peer = RawPeer::new().await?;
        let root_hash = publisher.publish(&content(4 * 1024)).await?;

        let session = peer.dial(&publisher.endpoint.profile, &root_hash).await?;
        // A Session starts as choking the peer, so this request is not answered, and it is not a violation either.
        RawPeer::send(
            &session,
            &ExchangeMessage::Request {
                block_hashes: vec![root_hash.clone()],
            },
        )
        .await?;
        RawPeer::send(&session, &ExchangeMessage::Status { want: true, choke: true }).await?;
        loop {
            match RawPeer::receive(&session).await? {
                ExchangeMessage::Status { choke: false, .. } => break,
                ExchangeMessage::Block { .. } | ExchangeMessage::HaveMap { .. } => panic!("a choked requester was answered"),
                _ => {}
            }
        }

        RawPeer::send(
            &session,
            &ExchangeMessage::Request {
                block_hashes: vec![root_hash.clone()],
            },
        )
        .await?;
        loop {
            if let ExchangeMessage::Block { block_hash, value } = RawPeer::receive(&session).await? {
                assert_eq!(block_hash, root_hash);
                assert_eq!(OmniHash::compute_hash(root_hash.typ.clone(), &value), root_hash);
                break;
            }
        }
        assert!(!publisher.ctx().is_excluded(&peer.id()));
        publisher.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_requested_hash_the_node_does_not_hold_is_answered_by_the_presence_bitmap() -> TestResult {
        let publisher = TestNode::new().await?;
        let peer = RawPeer::new().await?;
        let root_hash = publisher.publish(&content(4 * 1024)).await?;

        let session = peer.dial(&publisher.endpoint.profile, &root_hash).await?;
        RawPeer::send(&session, &ExchangeMessage::Status { want: true, choke: true }).await?;
        loop {
            if let ExchangeMessage::Status { choke: false, .. } = RawPeer::receive(&session).await? {
                break;
            }
        }
        let missing = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"not part of the file");
        RawPeer::send(
            &session,
            &ExchangeMessage::Request {
                block_hashes: vec![missing.clone(), root_hash.clone()],
            },
        )
        .await?;
        // The bitmap answers the whole request before the blocks: the first hash is absent, the second present.
        let mut have_map = false;
        let mut block = false;
        while !(have_map && block) {
            match RawPeer::receive(&session).await? {
                ExchangeMessage::HaveMap { bits } => {
                    assert_eq!(bits, vec![0b0000_0010]);
                    have_map = true;
                }
                ExchangeMessage::Block { block_hash, .. } => {
                    assert_eq!(block_hash, root_hash);
                    block = true;
                }
                _ => {}
            }
        }
        // Asking for a hash that is not part of the file is not a violation.
        assert!(!publisher.ctx().is_excluded(&peer.id()));
        publisher.shutdown().await;
        peer.endpoint.session_accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_root_hash_the_peer_does_not_provide_is_rejected_and_set_aside() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let unknown = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"nobody provides this");
        subscriber.knows(&[&publisher.endpoint.profile]);

        subscriber.subscribe(&unknown, "output.bin").await?;
        let ctx = subscriber.ctx().clone();
        let publisher_id = publisher.id();
        wait_for(|| {
            let (ctx, publisher_id, unknown) = (ctx.clone(), publisher_id.clone(), unknown.clone());
            async move { ctx.is_pair_set_aside(&publisher_id, &unknown) }
        })
        .await?;
        assert_eq!(publisher.ctx().registry.count_by_root_hash(&unknown), 0);
        publisher.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_session_neither_side_wants_is_closed_and_the_pair_is_set_aside() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        publisher.knows(&[&subscriber.endpoint.profile]);
        subscriber.knows(&[&publisher.endpoint.profile]);
        let root_hash = publisher.publish(&content(2 * 1024)).await?;

        let (id, _) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.wait_completed(&id).await?;

        // The subscriber has the file now, so neither side wants blocks, and the Session is closed instead of occupying a slot.
        let (publisher_ctx, subscriber_ctx) = (publisher.ctx().clone(), subscriber.ctx().clone());
        wait_for(|| {
            let (publisher_ctx, subscriber_ctx, root_hash) = (publisher_ctx.clone(), subscriber_ctx.clone(), root_hash.clone());
            async move { publisher_ctx.registry.count_by_root_hash(&root_hash) == 0 && subscriber_ctx.registry.count_by_root_hash(&root_hash) == 0 }
        })
        .await?;
        assert!(subscriber.ctx().is_pair_set_aside(&publisher.id(), &root_hash) || publisher.ctx().is_pair_set_aside(&subscriber.id(), &root_hash));
        publisher.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_node_answering_at_the_address_of_another_node_is_not_dialed() -> TestResult {
        let publisher = TestNode::new().await?;
        let subscriber = TestNode::new().await?;
        let content = content(4 * 1024);
        let root_hash = publisher.publish(&content).await?;
        // The lookup result names a node whose key is not the publisher's, at the publisher's address.
        let other_signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "other")?;
        let stale = NodeProfile::sign(&other_signer, publisher.endpoint.profile.addrs().to_vec(), Utc::now())?;
        subscriber.knows(&[&stale]);

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(subscriber.ctx().registry.count_by_root_hash(&root_hash), 0);
        assert_eq!(publisher.ctx().registry.count_by_root_hash(&root_hash), 0);

        subscriber.knows(&[&publisher.endpoint.profile]);
        subscriber.wait_completed(&id).await?;
        assert_eq!(tokio::fs::read(&output).await?, content);
        publisher.shutdown().await;
        subscriber.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_provided_root_hashes_are_advertised() -> TestResult {
        let publisher = TestNode::new().await?;
        let root_hash = publisher.publish(&content(1024)).await?;

        let locations = publisher.locations.clone().unwrap();
        wait_for(|| {
            let (locations, root_hash) = (locations.clone(), root_hash.clone());
            async move { locations.asset_keys() == vec![crate::model::AssetKey::file(root_hash)] }
        })
        .await?;
        publisher.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_file_advertised_through_node_finder_is_found_and_transferred() -> TestResult {
        let publisher = TestNode::with_node_finder(&[]).await?;
        let subscriber = TestNode::with_node_finder(&[&publisher.endpoint.profile]).await?;
        let content = content(32 * 1024);
        let root_hash = publisher.publish(&content).await?;

        let (id, output) = subscriber.subscribe(&root_hash, "output.bin").await?;
        subscriber.wait_completed(&id).await?;

        assert_eq!(tokio::fs::read(&output).await?, content);
        subscriber.shutdown().await;
        publisher.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn new_fails_when_state_directory_cannot_be_created() -> TestResult {
        let dir = tempfile::tempdir()?;
        let endpoint = Endpoint::new(dir.path(), &[SessionType::FileExchanger]).await?;
        let state_file = dir.path().join("state-file");
        tokio::fs::write(&state_file, b"").await?;

        let result = FileExchanger::new(
            endpoint.session_connector.clone(),
            endpoint.session_accepter.clone(),
            Arc::new(FixedLocationFinder::new(endpoint.profile.id().to_vec())),
            tsid_provider(),
            Arc::new(ClockUtc),
            Arc::new(SleeperImpl),
            Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)))),
            FileExchangerOption::new(state_file.join("state")),
            fast_intervals(),
        )
        .await;

        endpoint.session_accepter.shutdown().await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn new_shuts_down_publisher_when_subscriber_start_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let endpoint = Endpoint::new(dir.path(), &[SessionType::FileExchanger]).await?;
        // The publisher starts first, and the subscriber's directory is a file so that its start fails.
        let state_dir = dir.path().join("state");
        tokio::fs::create_dir_all(&state_dir).await?;
        tokio::fs::write(state_dir.join("file_subscriber"), b"").await?;
        let tsid_provider = tsid_provider();

        let result = FileExchanger::new(
            endpoint.session_connector.clone(),
            endpoint.session_accepter.clone(),
            Arc::new(FixedLocationFinder::new(endpoint.profile.id().to_vec())),
            tsid_provider.clone(),
            Arc::new(ClockUtc),
            Arc::new(SleeperImpl),
            Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)))),
            FileExchangerOption::new(state_dir.clone()),
            fast_intervals(),
        )
        .await;

        endpoint.session_accepter.shutdown().await;
        assert!(result.is_err());
        assert!(state_dir.join("file_publisher/blocks/LOCK").is_file());
        // The encoder ended and released its store, so the same RocksDB can be opened again.
        let publisher = FilePublisher::new(&state_dir.join("file_publisher"), tsid_provider, Arc::new(ClockUtc), Arc::new(SleeperImpl)).await?;
        publisher.shutdown().await;
        Ok(())
    }
}

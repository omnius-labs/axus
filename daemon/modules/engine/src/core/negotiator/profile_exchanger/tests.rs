use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::future::join_all;
use parking_lot::Mutex;
use rand::{
    SeedableRng as _,
    rngs::{ChaCha20Rng, SysRng},
};
use rand_core::UnwrapErr;
use testresult::TestResult;
use tokio_util::sync::CancellationToken;

use omnius_core_base::{
    clock::{Clock, ClockUtc},
    sleeper::SleeperImpl,
};
use omnius_core_omnikit::{generated::omni_sign::OmniSigner, model::omni_addr::OmniAddr};

use crate::{
    base::connection::{ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType},
    core::{
        identity::NodeIdentity,
        profile::LocalProfiles,
        session::{
            SessionAccepter, SessionConnector,
            model::{SessionOption, SessionType},
        },
    },
    generated::axus::profile::ExchangeMessage,
    model::{AssetKey, NodeProfile, Profile, ProfileId},
    protocol::profile::ExchangeMessageCodec,
};

use super::*;

const FAST_INTERVALS: ProfileExchangerIntervals = ProfileExchangerIntervals {
    receive_timeout: Duration::from_secs(2),
    accept_retry: Duration::from_millis(20),
};

/// What a node needs to be reached: the key it proves in the Session and the address it listens on.
struct Endpoint {
    session_accepter: Arc<SessionAccepter>,
    session_connector: Arc<SessionConnector>,
    profile: NodeProfile,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl Endpoint {
    async fn new(dir: &Path) -> TestResult<Self> {
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let sleeper: Arc<dyn omnius_core_base::sleeper::Sleeper + Send + Sync> = Arc::new(SleeperImpl);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let identity = NodeIdentity::load_or_create(&dir.join("identity")).await?;
        let signer: Arc<OmniSigner> = identity.signer();

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
        let session_accepter = Arc::new(
            SessionAccepter::new(
                tcp_accepter,
                signer.clone(),
                clock.clone(),
                sleeper,
                rng.clone(),
                &[SessionType::ProfileExchanger],
                SessionOption::default(),
            )
            .await,
        );
        let session_connector = Arc::new(SessionConnector::new(tcp_connector, signer.clone(), clock.clone(), rng, SessionOption::default()));
        let profile = NodeProfile::sign(&signer, addrs, clock.now())?;

        Ok(Self {
            session_accepter,
            session_connector,
            profile,
            clock,
        })
    }
}

struct TestNode {
    exchanger: ProfileExchanger,
    endpoint: Endpoint,
    local_profiles: Arc<LocalProfiles>,
    locations: Arc<FixedProfileLocationFinder>,
    dir: tempfile::TempDir,
}

impl TestNode {
    async fn new() -> TestResult<Self> {
        Self::with_intervals(FAST_INTERVALS).await
    }

    async fn with_intervals(intervals: ProfileExchangerIntervals) -> TestResult<Self> {
        let dir = tempfile::tempdir()?;
        let endpoint = Endpoint::new(dir.path()).await?;
        let local_profiles = Arc::new(LocalProfiles::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
        let locations = Arc::new(FixedProfileLocationFinder::new(endpoint.profile.id().to_vec()));
        let exchanger = ProfileExchanger::new(
            local_profiles.clone(),
            endpoint.session_connector.clone(),
            endpoint.session_accepter.clone(),
            locations.clone(),
            endpoint.clock.clone(),
            ProfileExchangerOption::default(),
            intervals,
        )
        .await?;

        Ok(Self {
            exchanger,
            endpoint,
            local_profiles,
            locations,
            dir,
        })
    }

    async fn active(&self) -> TestResult<Profile> {
        Ok(self.local_profiles.active().await?.expect("a fresh node has an active profile"))
    }

    fn known(&self, peers: &[&NodeProfile]) {
        self.locations.set_locations(peers.iter().map(|peer| (*peer).clone()).collect());
    }

    async fn shutdown(&self) {
        self.exchanger.shutdown().await;
        self.local_profiles.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_startup_failure_leaves_no_worker() -> TestResult {
    let dir = tempfile::tempdir()?;
    let endpoint = Endpoint::new(dir.path()).await?;
    let local_profiles = Arc::new(LocalProfiles::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
    local_profiles.shutdown().await;
    let locations = Arc::new(FixedProfileLocationFinder::new(endpoint.profile.id().to_vec()));

    let result = ProfileExchanger::new(
        local_profiles,
        endpoint.session_connector.clone(),
        endpoint.session_accepter.clone(),
        locations,
        endpoint.clock.clone(),
        ProfileExchangerOption::default(),
        FAST_INTERVALS,
    )
    .await;

    assert!(result.is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_advertises_owned_profiles() -> TestResult {
    let node = TestNode::new().await?;
    let active = node.active().await?;

    assert_eq!(node.locations.provided(), vec![AssetKey::profile(active.id())]);
    node.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_fetches_an_owned_profile_and_stores_it() -> TestResult {
    let server = TestNode::new().await?;
    let client = TestNode::new().await?;
    let profile = server.active().await?;
    client.known(&[&server.endpoint.profile]);

    let fetched = client.exchanger.fetch(profile.id()).await?;
    assert_eq!(fetched, profile);
    assert_eq!(client.exchanger.stored(profile.id()).await?, Some(profile.clone()));

    let profile_dir = client.dir.path().join("profile");
    client.shutdown().await;
    let reopened = LocalProfiles::open(&profile_dir, Arc::new(ClockUtc)).await?;
    assert_eq!(reopened.get(profile.id()).await?, None);
    assert_eq!(reopened.fetched(profile.id()).await?.expect("the fetched profile is stored").profile, profile);
    reopened.shutdown().await;

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_serves_only_owned_profiles() -> TestResult {
    let server = TestNode::new().await?;
    let client = TestNode::new().await?;
    let third = TestNode::new().await?;
    let profile = server.active().await?;
    client.known(&[&server.endpoint.profile]);
    client.exchanger.fetch(profile.id()).await?;
    third.known(&[&client.endpoint.profile]);

    assert!(third.exchanger.fetch(profile.id()).await.is_err());
    assert!(third.exchanger.fetch(&ProfileId::new("Missing", b"missing")?).await.is_err());
    assert_eq!(third.exchanger.stored(profile.id()).await?, None);

    third.shutdown().await;
    client.shutdown().await;
    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_tries_the_next_location_after_a_bad_body() -> TestResult {
    let dir = tempfile::tempdir()?;
    let bad = Endpoint::new(dir.path()).await?;
    let bad_local = Arc::new(LocalProfiles::open(&dir.path().join("profile"), bad.clock.clone()).await?);
    let bad_profile = bad_local.active().await?.expect("a fresh node has an active profile");
    let honest = TestNode::new().await?;
    let client = TestNode::new().await?;
    let honest_profile = honest.active().await?;
    client.known(&[&bad.profile, &honest.endpoint.profile]);

    let accepter = bad.session_accepter.clone();
    let bad_server = tokio::spawn(async move {
        let session = accepter.accept(&SessionType::ProfileExchanger).await?;
        session.stream.set_receive_timeout(Duration::from_secs(5));
        let _ = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await?;
        session
            .stream
            .sender
            .lock()
            .await
            .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::Response { profile: bad_profile.to_wire() })
            .await?;
        Result::Ok(())
    });

    let fetched = client.exchanger.fetch(honest_profile.id()).await?;
    assert_eq!(fetched, honest_profile);
    bad_server.await??;
    bad_local.shutdown().await;

    client.shutdown().await;
    honest.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_closes_the_session_for_a_malformed_request() -> TestResult {
    let server = TestNode::new().await?;
    let session = server
        .endpoint
        .session_connector
        .connect(&server.endpoint.profile.addrs()[0], &SessionType::ProfileExchanger)
        .await?;
    session
        .stream
        .sender
        .lock()
        .await
        .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::Request {
            profile_id: "not-a-profile-id".to_owned(),
        })
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(5), session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>()).await;
    assert!(result.is_ok_and(|result| result.is_err()));

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_shares_one_session_limit_between_accept_and_fetch() -> TestResult {
    let node = TestNode::new().await?;

    assert!(Arc::ptr_eq(&node.exchanger.slots, node.exchanger.task_accepter.session_slots()));
    // The accepter reserves one permit while it waits for the next connection.
    assert_eq!(node.exchanger.task_accepter.session_slots().available_permits(), 7);

    node.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn profile_exchanger_limits_concurrent_fetch_sessions() -> TestResult {
    let dir = tempfile::tempdir()?;
    let endpoint = Endpoint::new(dir.path()).await?;
    let client_dir = tempfile::tempdir()?;
    let client_endpoint = Endpoint::new(client_dir.path()).await?;
    let server_profiles = Arc::new(LocalProfiles::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let accepter = endpoint.session_accepter.clone();
    let server_cancellation = CancellationToken::new();
    let server_task = {
        let active = active.clone();
        let max_active = max_active.clone();
        let server_cancellation = server_cancellation.clone();
        tokio::spawn(async move {
            let mut workers = Vec::new();
            loop {
                let session = tokio::select! {
                    session = accepter.accept(&SessionType::ProfileExchanger) => session?,
                    _ = server_cancellation.cancelled() => break,
                };
                let active = active.clone();
                let max_active = max_active.clone();
                workers.push(tokio::spawn(async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(current, Ordering::SeqCst);
                    let _ = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    let _ = session
                        .stream
                        .sender
                        .lock()
                        .await
                        .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::NotFound { profile_id: "missing".to_owned() })
                        .await;
                }));
            }
            for worker in workers {
                let _ = worker.await;
            }
            Result::Ok(())
        })
    };

    let client_profiles = Arc::new(LocalProfiles::open(&client_dir.path().join("profile"), client_endpoint.clock.clone()).await?);
    let locations = Arc::new(FixedProfileLocationFinder::new(client_endpoint.profile.id().to_vec()));
    locations.set_locations(vec![endpoint.profile.clone()]);
    let exchanger = ProfileExchanger::new(
        client_profiles.clone(),
        client_endpoint.session_connector.clone(),
        client_endpoint.session_accepter.clone(),
        locations,
        client_endpoint.clock.clone(),
        ProfileExchangerOption { max_session_count: 4 },
        FAST_INTERVALS,
    )
    .await?;

    let ids = (0..6).map(|index| ProfileId::new(&format!("P{index}"), b"key")).collect::<Result<Vec<_>>>()?;
    let results = join_all(ids.iter().map(|id| exchanger.fetch(id))).await;
    assert!(results.iter().all(|result| result.is_err()));
    assert!(max_active.load(Ordering::SeqCst) > 1);
    assert!(max_active.load(Ordering::SeqCst) <= 4);

    exchanger.shutdown().await;
    server_cancellation.cancel();
    server_task.await??;
    client_profiles.shutdown().await;
    server_profiles.shutdown().await;
    Ok(())
}

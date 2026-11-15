use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use omnius_core_base::clock::Clock;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::session::{
        SessionAccepter, SessionConnector,
        model::{Session, SessionType},
    },
    generated::axus::profile::ExchangeMessage,
    model::{AssetKey, Profile, ProfileId},
    prelude::*,
    protocol::profile::ExchangeMessageCodec,
};

use super::*;

#[derive(Debug, Clone)]
pub struct ProfileExchangerOption {
    /// Sessions dialed and accepted together.
    pub max_session_count: usize,
}

impl Default for ProfileExchangerOption {
    fn default() -> Self {
        Self { max_session_count: 8 }
    }
}

impl ProfileExchangerOption {
    /// The semaphore shared by the accepter and the fetch path.
    pub fn session_slots(&self) -> Arc<Semaphore> {
        Arc::new(Semaphore::new(self.max_session_count))
    }
}

#[derive(Debug, Clone)]
pub struct ProfileExchangerIntervals {
    pub receive_timeout: Duration,
    pub accept_retry: Duration,
}

impl Default for ProfileExchangerIntervals {
    fn default() -> Self {
        Self {
            receive_timeout: Duration::from_secs(30),
            accept_retry: Duration::from_secs(1),
        }
    }
}

/// Finds and fetches the signed body of a Profile by its ID, and serves the Profiles this node owns.
pub struct ProfileExchanger {
    profile_store: Arc<ProfileStore>,
    session_connector: Arc<SessionConnector>,
    location_finder: Arc<dyn ProfileLocationFinder>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    intervals: ProfileExchangerIntervals,
    slots: Arc<Semaphore>,
    cancellation_token: CancellationToken,
    task_accepter: Arc<TaskAccepter>,
}

impl ProfileExchanger {
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        profile_store: Arc<ProfileStore>,
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        location_finder: Arc<dyn ProfileLocationFinder>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        option: ProfileExchangerOption,
        intervals: ProfileExchangerIntervals,
    ) -> Result<Self> {
        Self::new_with_slots(
            profile_store,
            session_connector,
            session_accepter,
            location_finder,
            clock,
            option.session_slots(),
            intervals,
        )
        .await
    }

    /// Uses the same semaphore as the SessionAccepter, so the accept and fetch paths share one limit.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_slots(
        profile_store: Arc<ProfileStore>,
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        location_finder: Arc<dyn ProfileLocationFinder>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        slots: Arc<Semaphore>,
        intervals: ProfileExchangerIntervals,
    ) -> Result<Self> {
        let cancellation_token = CancellationToken::new();
        let task_accepter = TaskAccepter::new(profile_store.clone(), session_accepter, intervals.clone(), cancellation_token.clone());
        let exchanger = Self {
            profile_store,
            session_connector,
            location_finder,
            clock,
            intervals,
            slots,
            cancellation_token,
            task_accepter,
        };

        if let Err(error) = exchanger.refresh_provided().await {
            exchanger.shutdown().await;
            return Err(error);
        }

        Ok(exchanger)
    }

    pub fn my_node_id(&self) -> Vec<u8> {
        self.location_finder.my_node_id()
    }

    /// Replaces the Profile AssetKeys advertised by push with the Profiles this node owns.
    pub async fn refresh_provided(&self) -> Result<()> {
        let asset_keys = self.profile_store.list().await?.into_iter().map(|profile| AssetKey::profile(profile.id())).collect();
        self.location_finder.set_provided(asset_keys);
        Ok(())
    }

    /// Reads a Profile this node owns or has fetched, without network access.
    pub async fn stored(&self, id: &ProfileId) -> Result<Option<Profile>> {
        if let Some(profile) = self.profile_store.get(id).await? {
            return Ok(Some(profile));
        }
        Ok(self.profile_store.fetched(id).await?.map(|fetched| fetched.profile))
    }

    /// Fetches a Profile by ID from the locations NodeFinder returns.
    ///
    /// An owned Profile is returned directly. NotFound, a bad body, and a stale or equivocating
    /// snapshot move to the next location; the serving node is not excluded for a bad body.
    pub async fn fetch(&self, id: &ProfileId) -> Result<Profile> {
        if let Some(profile) = self.profile_store.get(id).await? {
            return Ok(profile);
        }

        let locations = self.location_finder.find(&AssetKey::profile(id)).await?;
        if locations.is_empty() {
            return Err(Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {id}")));
        }

        let mut last_error = None;
        for node_profile in locations {
            if node_profile.id() == self.my_node_id() {
                continue;
            }
            for addr in node_profile.addrs() {
                let permit = self.slots.clone().acquire_owned().await.map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;
                let session = match self.session_connector.connect(addr, &SessionType::ProfileExchanger).await {
                    Ok(session) => session,
                    Err(error) => {
                        last_error = Some(error);
                        continue;
                    }
                };
                if !session.is_signed_by(&node_profile) {
                    warn!(address = addr.to_string(), "the peer's key does not match the node profile");
                    drop(permit);
                    continue;
                }

                let result = self.fetch_from_session(&session, id).await;
                drop(session);
                drop(permit);

                match result {
                    Ok(profile) => return Ok(profile),
                    Err(error) => last_error = Some(error),
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {id}"))))
    }

    async fn fetch_from_session(&self, session: &Session, id: &ProfileId) -> Result<Profile> {
        session.stream.set_receive_timeout(self.intervals.receive_timeout);

        let request = ExchangeMessage::Request { profile_id: id.to_string() };
        session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(&request).await?;

        let response = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await?;
        let profile = match response {
            ExchangeMessage::Response { profile } => profile,
            ExchangeMessage::NotFound { profile_id } => {
                return Err(Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {profile_id}")));
            }
            _ => return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of a profile response")),
        };

        let profile = Profile::verified(profile)?;
        if profile.id() != id {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the response carries another profile id"));
        }

        match self.profile_store.store_fetched(&profile, self.clock.now()).await? {
            FetchedProfileReplaceResult::Replaced | FetchedProfileReplaceResult::Unchanged => Ok(profile),
            FetchedProfileReplaceResult::Stale => Err(Error::new(ErrorKind::Reject).with_message("the fetched profile is older than the stored one")),
            FetchedProfileReplaceResult::Equivocation => Err(Error::new(ErrorKind::Reject).with_message("the fetched profile equivocates")),
        }
    }
}

#[async_trait]
impl Shutdown for ProfileExchanger {
    async fn shutdown(&self) {
        self.cancellation_token.cancel();
        self.task_accepter.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use chrono::{DateTime, Utc};
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
    use omnius_core_omnikit::{
        generated::omni_sign::{OmniSignType, OmniSigner},
        model::omni_addr::OmniAddr,
    };

    use crate::{
        base::connection::{ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType},
        core::{
            identity::NodeIdentity,
            session::{
                SessionAccepter, SessionConnector,
                model::{SessionOption, SessionType},
            },
        },
        generated::axus::{model::Profile as WireProfile, profile::ExchangeMessage},
        model::{AssetKey, NodeProfile, Profile, ProfileDraft, ProfileId},
        protocol::profile::ExchangeMessageCodec,
    };

    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

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
            Self::with_slots(dir, None).await
        }

        async fn with_slots(dir: &Path, slots: Option<Arc<tokio::sync::Semaphore>>) -> TestResult<Self> {
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
            let accept_limits = slots.into_iter().map(|slots| (SessionType::ProfileExchanger, slots)).collect::<Vec<_>>();
            let session_accepter = Arc::new(
                SessionAccepter::new_with_limits(
                    tcp_accepter,
                    signer.clone(),
                    clock.clone(),
                    sleeper,
                    rng.clone(),
                    &[SessionType::ProfileExchanger],
                    SessionOption::default(),
                    &accept_limits,
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

        fn spawn_profile_server(&self, profile: WireProfile) -> tokio::task::JoinHandle<Result<()>> {
            let accepter = self.session_accepter.clone();
            tokio::spawn(async move {
                let session = accepter.accept(&SessionType::ProfileExchanger).await?;
                session.stream.set_receive_timeout(Duration::from_secs(5));
                let _ = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await?;
                session
                    .stream
                    .sender
                    .lock()
                    .await
                    .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::Response { profile })
                    .await?;
                Result::Ok(())
            })
        }

        fn spawn_not_found_server(&self) -> tokio::task::JoinHandle<Result<()>> {
            let accepter = self.session_accepter.clone();
            tokio::spawn(async move {
                let session = accepter.accept(&SessionType::ProfileExchanger).await?;
                session.stream.set_receive_timeout(Duration::from_secs(5));
                let message = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await?;
                let ExchangeMessage::Request { profile_id } = message else {
                    return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of a profile request"));
                };
                session
                    .stream
                    .sender
                    .lock()
                    .await
                    .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::NotFound { profile_id })
                    .await?;
                Result::Ok(())
            })
        }
    }

    struct TestNode {
        exchanger: ProfileExchanger,
        endpoint: Endpoint,
        profile_store: Arc<ProfileStore>,
        locations: Arc<FixedProfileLocationFinder>,
        dir: tempfile::TempDir,
    }

    impl TestNode {
        async fn new() -> TestResult<Self> {
            Self::with_intervals(FAST_INTERVALS).await
        }

        async fn with_intervals(intervals: ProfileExchangerIntervals) -> TestResult<Self> {
            let dir = tempfile::tempdir()?;
            let slots = ProfileExchangerOption::default().session_slots();
            let endpoint = Endpoint::with_slots(dir.path(), Some(slots.clone())).await?;
            let profile_store = Arc::new(ProfileStore::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
            let locations = Arc::new(FixedProfileLocationFinder::new(endpoint.profile.id().to_vec()));
            let exchanger = ProfileExchanger::new_with_slots(
                profile_store.clone(),
                endpoint.session_connector.clone(),
                endpoint.session_accepter.clone(),
                locations.clone(),
                endpoint.clock.clone(),
                slots,
                intervals,
            )
            .await?;

            Ok(Self {
                exchanger,
                endpoint,
                profile_store,
                locations,
                dir,
            })
        }

        async fn active(&self) -> TestResult<Profile> {
            Ok(self.profile_store.active().await?.expect("a fresh node has an active profile"))
        }

        fn known(&self, peers: &[&NodeProfile]) {
            self.locations.set_locations(peers.iter().map(|peer| (*peer).clone()).collect());
        }

        async fn shutdown(&self) {
            self.exchanger.shutdown().await;
            self.profile_store.shutdown().await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_startup_failure_leaves_no_worker() -> TestResult {
        let dir = tempfile::tempdir()?;
        let endpoint = Endpoint::new(dir.path()).await?;
        let profile_store = Arc::new(ProfileStore::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
        profile_store.shutdown().await;
        let locations = Arc::new(FixedProfileLocationFinder::new(endpoint.profile.id().to_vec()));

        let result = ProfileExchanger::new(
            profile_store,
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
        let reopened = ProfileStore::open(&profile_dir, Arc::new(ClockUtc)).await?;
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
    async fn profile_exchanger_tries_the_next_location_after_not_found() -> TestResult {
        let missing_dir = tempfile::tempdir()?;
        let missing = Endpoint::new(missing_dir.path()).await?;
        let good_dir = tempfile::tempdir()?;
        let good = Endpoint::new(good_dir.path()).await?;
        let client = TestNode::new().await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Remote")?;
        let profile = Profile::sign(&signer, 1, at(1_000_000), ProfileDraft::default())?;

        let missing_server = missing.spawn_not_found_server();
        let good_server = good.spawn_profile_server(profile.to_wire());
        client.known(&[&missing.profile, &good.profile]);

        assert_eq!(client.exchanger.fetch(profile.id()).await?, profile);
        assert_eq!(client.exchanger.stored(profile.id()).await?, Some(profile));
        missing_server.await??;
        good_server.await??;

        client.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_tries_the_next_location_after_a_bad_body() -> TestResult {
        let dir = tempfile::tempdir()?;
        let bad = Endpoint::new(dir.path()).await?;
        let bad_store = Arc::new(ProfileStore::open(&dir.path().join("profile"), bad.clock.clone()).await?);
        let bad_profile = bad_store.active().await?.expect("a fresh node has an active profile");
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
        bad_store.shutdown().await;

        client.shutdown().await;
        honest.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_tries_the_next_location_after_a_bad_signature() -> TestResult {
        let bad_dir = tempfile::tempdir()?;
        let bad = Endpoint::new(bad_dir.path()).await?;
        let good_dir = tempfile::tempdir()?;
        let good = Endpoint::new(good_dir.path()).await?;
        let client = TestNode::new().await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Remote")?;
        let profile = Profile::sign(&signer, 1, at(1_000_000), ProfileDraft::default())?;
        let mut tampered = profile.to_wire();
        tampered.signature[0] ^= 1;

        let bad_server = bad.spawn_profile_server(tampered);
        let good_server = good.spawn_profile_server(profile.to_wire());
        client.known(&[&bad.profile, &good.profile]);

        assert_eq!(client.exchanger.fetch(profile.id()).await?, profile);
        assert_eq!(client.exchanger.stored(profile.id()).await?, Some(profile));
        bad_server.await??;
        good_server.await??;

        client.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_tries_the_next_location_after_a_stale_snapshot() -> TestResult {
        let bad_dir = tempfile::tempdir()?;
        let bad = Endpoint::new(bad_dir.path()).await?;
        let good_dir = tempfile::tempdir()?;
        let good = Endpoint::new(good_dir.path()).await?;
        let client = TestNode::new().await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Remote")?;
        let older = Profile::sign(&signer, 1, at(1_000_000), ProfileDraft::default())?;
        let newer = Profile::sign(&signer, 2, at(1_000_001), ProfileDraft::default())?;
        client.profile_store.store_fetched(&newer, at(1_000_002)).await?;

        let bad_server = bad.spawn_profile_server(older.to_wire());
        let good_server = good.spawn_profile_server(newer.to_wire());
        client.known(&[&bad.profile, &good.profile]);

        assert_eq!(client.exchanger.fetch(newer.id()).await?, newer);
        assert_eq!(client.exchanger.stored(newer.id()).await?, Some(newer));
        bad_server.await??;
        good_server.await??;

        client.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_tries_the_next_location_after_an_equivocation() -> TestResult {
        let bad_dir = tempfile::tempdir()?;
        let bad = Endpoint::new(bad_dir.path()).await?;
        let good_dir = tempfile::tempdir()?;
        let good = Endpoint::new(good_dir.path()).await?;
        let client = TestNode::new().await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Remote")?;
        let first = Profile::sign(&signer, 1, at(1_000_000), ProfileDraft::default())?;
        let other = ProfileId::new("Other", b"other-public-key")?;
        let second = Profile::sign(
            &signer,
            1,
            at(1_000_000),
            ProfileDraft {
                trusted: vec![other],
                ..Default::default()
            },
        )?;
        assert_ne!(first, second);
        client.profile_store.store_fetched(&first, at(1_000_002)).await?;

        let bad_server = bad.spawn_profile_server(second.to_wire());
        let good_server = good.spawn_profile_server(first.to_wire());
        client.known(&[&bad.profile, &good.profile]);

        assert_eq!(client.exchanger.fetch(first.id()).await?, first);
        assert_eq!(client.exchanger.stored(first.id()).await?, Some(first));
        bad_server.await??;
        good_server.await??;

        client.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_shutdown_joins_an_active_serve_worker() -> TestResult {
        let server = TestNode::new().await?;
        let _session = server
            .endpoint
            .session_connector
            .connect(&server.endpoint.profile.addrs()[0], &SessionType::ProfileExchanger)
            .await?;

        tokio::time::timeout(Duration::from_secs(5), async {
            while server.exchanger.task_accepter.active_worker_count().await == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        tokio::time::timeout(Duration::from_secs(5), server.shutdown()).await?;
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

        let accept_limit = node
            .endpoint
            .session_accepter
            .accept_limit(&SessionType::ProfileExchanger)
            .expect("the accepter has the ProfileExchanger limit");
        assert!(Arc::ptr_eq(&node.exchanger.slots, &accept_limit));
        assert_eq!(accept_limit.available_permits(), 8);

        node.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn profile_exchanger_limits_concurrent_fetch_sessions() -> TestResult {
        let dir = tempfile::tempdir()?;
        let endpoint = Endpoint::new(dir.path()).await?;
        let client_dir = tempfile::tempdir()?;
        let client_endpoint = Endpoint::new(client_dir.path()).await?;
        let server_profiles = Arc::new(ProfileStore::open(&dir.path().join("profile"), endpoint.clock.clone()).await?);
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

        let client_profiles = Arc::new(ProfileStore::open(&client_dir.path().join("profile"), client_endpoint.clock.clone()).await?);
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
}

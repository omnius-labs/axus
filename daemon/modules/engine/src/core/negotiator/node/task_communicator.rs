use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use enumflags2::make_bitflags;
use parking_lot::Mutex;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, mpsc},
    task::JoinHandle,
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::{
        negotiator::node::{AnnounceLocation, NodeFinderVersion, NodeMessage},
        session::{
            SessionConnector,
            model::{Session, SessionHandshakeType, SessionType},
        },
    },
    model::{AssetKey, NodeProfile},
    prelude::*,
    protocol::node::NodeMessageCodec,
};

use super::*;

#[derive(Clone)]
pub struct TaskCommunicator {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<SessionRegistry>,
    session_connector: Arc<SessionConnector>,
    node_profile_repo: Arc<NodeFinderRepo>,
    location_cache: Arc<Mutex<LocationCache>>,
    lookup_registry: Arc<LookupRegistry>,
    session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    communicate_join_handles: Arc<TokioMutex<Vec<JoinHandle<()>>>>,
    cancellation_token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskCommunicator {
    async fn shutdown(&self) {
        self.cancellation_token.cancel();

        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }

        // cancel の後に確立された Session のタスクも残らないよう、空になるまで join する
        loop {
            let join_handles: Vec<JoinHandle<()>> = self.communicate_join_handles.lock().await.drain(..).collect();
            if join_handles.is_empty() {
                break;
            }
            for join_handle in join_handles {
                let _ = join_handle.await;
            }
        }
    }
}

impl TaskCommunicator {
    /// The nodes one advertisement is relayed to, and the nodes one lookup asks at a time.
    pub const FAN_OUT: usize = 3;

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<SessionRegistry>,
        session_connector: Arc<SessionConnector>,
        node_profile_repo: Arc<NodeFinderRepo>,
        location_cache: Arc<Mutex<LocationCache>>,
        lookup_registry: Arc<LookupRegistry>,
        session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let cancellation_token = CancellationToken::new();

        let v = Arc::new(Self {
            my_node_profile,
            sessions,
            session_connector,
            node_profile_repo,
            location_cache,
            lookup_registry,
            session_receiver,
            clock,
            sleeper,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
            communicate_join_handles: Arc::new(TokioMutex::new(Vec::new())),
            cancellation_token: cancellation_token.clone(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            while !this.cancellation_token.is_cancelled() {
                // 終了済みのタスクを削除
                this.communicate_join_handles.lock().await.retain(|join_handle| !join_handle.is_finished());

                let status = select! {
                    status = async { this.session_receiver.lock().await.recv().await } => status,
                    _ = this.cancellation_token.cancelled() => break,
                };
                if let Some(status) = status {
                    let communicator = this.clone();
                    let join_handle = tokio::spawn(async move {
                        let res = communicator.communicate(status).await;
                        if let Err(e) = res {
                            warn!(error_message = e.to_string(), "communicate failed");
                        }
                    });
                    this.communicate_join_handles.lock().await.push(join_handle);
                }
            }
        }));

        Ok(())
    }

    async fn communicate(self: Arc<Self>, status: SessionStatus) -> Result<()> {
        // 確立後の後始末は supervisor が持つので、ここでは相手が閉じるまで待つ
        let status = self.establish(status).await?;
        status.cancellation_token.cancelled().await;

        Ok(())
    }

    /// Dials a node and registers the established Session.
    ///
    /// The lookup uses this when the closest node it needs is not connected; the Session then behaves like one the connector dialed.
    pub async fn connect(self: &Arc<Self>, node_profile: &NodeProfile) -> Result<Arc<SessionStatus>> {
        // 上限は Connected の数で数える。connector と同じ規則なので、lookup も同じ枠を共有する
        if self.sessions.count_by_handshake_type(&SessionHandshakeType::Connected).await >= self.option.max_connected_session_count {
            return Err(Error::new(ErrorKind::NotConnected).with_message("the connected Session limit is reached"));
        }

        let mut last_error = None;
        for addr in node_profile.addrs() {
            let Ok(session) = self.session_connector.connect(addr, &SessionType::NodeFinder).await else {
                continue;
            };
            // 公開済みの addr は古くなり得るため、別の node が応答した接続は採用しない
            if !session.is_signed_by(node_profile) {
                warn!(address = addr.to_string(), "the peer's key does not match the node profile");
                continue;
            }
            match self.establish(SessionStatus::new(session)).await {
                Ok(status) => return Ok(status),
                // 既に同じ相手と繋がっているなら、その Session を使う
                Err(e) if e.kind() == &ErrorKind::AlreadyExists => {
                    if let Some((_, status)) = self.sessions.statuses().await.into_iter().find(|(id, _)| id == node_profile.id()) {
                        return Ok(status);
                    }
                    last_error = Some(e);
                }
                // 1 つの addr で handshake が失敗しても、残りの addr は試す
                Err(e) => last_error = Some(e),
            }
        }

        self.node_profile_repo.mark_unreachable(node_profile.id()).await?;

        Err(last_error.unwrap_or_else(|| Error::new(ErrorKind::NotConnected).with_message("no address of the node accepted a connection")))
    }

    /// Finishes the handshake, registers the Session, and starts its writer, reader, and cleanup.
    async fn establish(self: &Arc<Self>, status: SessionStatus) -> Result<Arc<SessionStatus>> {
        let my_node_profile = self.my_node_profile.lock().clone();
        // 登録前なので、cancel されたら後始末なしで終える
        let other_node_profile = select! {
            res = Self::handshake(&status.session, &my_node_profile, self.option.intervals.receive_timeout()) => res?,
            _ = self.cancellation_token.cancelled() => return Err(Error::new(ErrorKind::NotConnected).with_message("shutdown")),
        };

        let other_node_profile_uri = other_node_profile.to_uri()?;
        // handshake まで終えた相手だけが到達可能。profile を保存してから到達時刻を記録する
        self.node_profile_repo.insert_or_update_node_profiles(&[&other_node_profile]).await?;
        self.node_profile_repo.mark_reachable(other_node_profile.id()).await?;
        *status.node_profile.lock() = Some(other_node_profile.clone());
        let other_node_id = other_node_profile.id().to_vec();

        let status = Arc::new(status);

        match self.sessions.register(my_node_profile.id(), other_node_profile.id(), &status).await {
            Registration::Added => {}
            Registration::Replaced => info!(node_profile = other_node_profile_uri, "Session replaced"),
            Registration::Rejected => return Err(Error::new(ErrorKind::AlreadyExists).with_message("Session already exists")),
        }

        info!(node_profile = other_node_profile_uri, "Session established");

        let s = self.clone().send(status.clone()).await;
        let r = self.clone().receive(status.clone(), other_node_id.clone()).await;
        let c = self.clone().supervise(status.clone(), other_node_id).await;
        let mut handles = self.communicate_join_handles.lock().await;
        handles.push(s);
        handles.push(r);
        handles.push(c);

        Ok(status)
    }

    /// Removes the Session once its writer or reader ends.
    async fn supervise(self: Arc<Self>, status: Arc<SessionStatus>, other_node_id: Vec<u8>) -> JoinHandle<()> {
        tokio::spawn(async move {
            select! {
                _ = status.cancellation_token.cancelled() => {}
                _ = self.cancellation_token.cancelled() => status.cancellation_token.cancel(),
            }
            self.sessions.unregister(&other_node_id, &status).await;
            info!("Session closed");
        })
    }

    pub async fn handshake(session: &Session, node_profile: &NodeProfile, receive_timeout: Duration) -> Result<NodeProfile> {
        session.stream.set_receive_timeout(receive_timeout);
        let send_hello_message = NodeMessage::Hello {
            version: make_bitflags!(NodeFinderVersion::V1),
        };
        let received_hello_message = Self::exchange_message(session, &send_hello_message).await?;

        let NodeMessage::Hello { version } = received_hello_message else {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("expected the Hello message"));
        };
        let version = make_bitflags!(NodeFinderVersion::V1) & version;

        if version.contains(NodeFinderVersion::V1) {
            let send_profile_message = NodeMessage::Profile {
                node_profile: node_profile.clone(),
            };
            let received_profile_message = Self::exchange_message(session, &send_profile_message).await?;
            let NodeMessage::Profile { node_profile: received } = received_profile_message else {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("expected the Profile message"));
            };

            if received.id() == node_profile.id() {
                return Err(Error::new(ErrorKind::Reject).with_message("connected to self"));
            }

            // node ID は公開鍵から導出するため、Session で署名を確かめた鍵と一致すれば ID も相手のものと確定する
            if received.public_key() != session.cert.public_key.as_slice() {
                return Err(Error::new(ErrorKind::Reject).with_message("node profile does not match the session certificate"));
            }

            Ok(received)
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("invalid version: {}", version.bits())))
        }
    }

    async fn exchange_message(session: &Session, message: &NodeMessage) -> Result<NodeMessage> {
        session.stream.sender.lock().await.send_message_with::<NodeMessageCodec>(message).await?;
        session.stream.receiver.lock().await.recv_message_with::<NodeMessageCodec>().await
    }

    async fn send(self: Arc<Self>, status: Arc<SessionStatus>) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut outbox = status.take_outbox_receiver().await.expect("the writer owns the outbox");
            let f = async {
                loop {
                    // 送るものが無い周期は Status で生存を知らせる
                    let message = select! {
                        message = outbox.recv() => match message {
                            Some(message) => message,
                            None => return Ok::<(), Error>(()),
                        },
                        _ = this.sleeper.sleep(this.option.intervals.communicate) => NodeMessage::Status,
                    };
                    status.session.stream.sender.lock().await.send_message_with::<NodeMessageCodec>(&message).await?;
                }
            };
            if let Err(e) = select! {
                res = f => res,
                _ = this.cancellation_token.cancelled() => Ok(()),
                _ = status.cancellation_token.cancelled() => Ok(()),
            } {
                warn!(error_message = e.to_string(), "send failed");
            }
            status.cancellation_token.cancel();
        })
    }

    async fn receive(self: Arc<Self>, status: Arc<SessionStatus>, peer_id: Vec<u8>) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let f = async {
                let communicate = this.option.intervals.communicate;
                let receive_timeout = this.option.intervals.receive_timeout();
                loop {
                    let res = async {
                        let message = status.session.stream.receiver.lock().await.recv_message_with::<NodeMessageCodec>().await?;
                        // 受信した時点を基準にし、保存処理にかかった時間で期限を延ばさない
                        let received_at = Instant::now();
                        let wrote = timeout_at(received_at + receive_timeout, this.dispatch(&status, &peer_id, message))
                            .await
                            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
                        // 周期より速く送る相手に保存処理を繰り返させない。書き込まない Status と lookup は待たせない
                        if wrote {
                            this.sleeper.sleep(communicate.saturating_sub(received_at.elapsed())).await;
                        }
                        Ok::<(), Error>(())
                    }
                    .await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "receive failed");
                        return;
                    }
                }
            };
            select! {
                _ = f => {}
                _ = this.cancellation_token.cancelled() => {}
                _ = status.cancellation_token.cancelled() => {}
            }
            status.cancellation_token.cancel();
        })
    }

    /// Returns whether the message wrote to the store or the cache, which decides whether the cycle wait applies.
    async fn dispatch(&self, status: &Arc<SessionStatus>, peer_id: &[u8], message: NodeMessage) -> Result<bool> {
        let mut wrote = false;
        match message {
            NodeMessage::Status => {}
            NodeMessage::AnnounceProfiles { node_profiles } => {
                let now = self.clock.now();
                let node_profiles: Vec<&NodeProfile> = node_profiles
                    .iter()
                    .filter(|profile| profile.is_updated_within_skew(now))
                    .map(|profile| profile.as_ref())
                    .collect();
                self.node_profile_repo.insert_or_update_node_profiles(&node_profiles).await?;
                wrote = true;
            }
            NodeMessage::AnnounceLocations { locations } => {
                let mut relayed: HashMap<Arc<AssetKey>, AnnounceLocation> = HashMap::new();
                {
                    let mut location_cache = self.location_cache.lock();
                    for (asset_key, location) in locations {
                        location_cache.insert(asset_key.clone(), location.node_profiles.iter().cloned());
                        // A hop of zero ends the advertisement; otherwise relay it once to the nodes closest to the key.
                        if location.hops > 0 {
                            relayed.insert(
                                asset_key,
                                AnnounceLocation {
                                    hops: location.hops - 1,
                                    node_profiles: location.node_profiles,
                                },
                            );
                        }
                    }
                }
                self.relay_locations(relayed, peer_id).await;
                wrote = true;
            }
            NodeMessage::LookupRequest { lookup_id, asset_key } => {
                let locations = self.location_cache.lock().get(&asset_key).to_vec();
                let closer_nodes = self.closest_nodes(&asset_key).await?;
                if let Err(e) = status
                    .outbox
                    .send(NodeMessage::LookupResponse {
                        lookup_id,
                        locations,
                        closer_nodes,
                    })
                    .await
                {
                    debug!(error_message = e.to_string(), "dropped a lookup answer because the outbox is closed");
                }
            }
            NodeMessage::LookupResponse {
                lookup_id,
                locations,
                closer_nodes,
            } => {
                let answer = LookupAnswer { locations, closer_nodes };
                self.lookup_registry.complete(peer_id, lookup_id, answer).await;
            }
            NodeMessage::Hello { .. } | NodeMessage::Profile { .. } => {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected handshake message"));
            }
        }

        Ok(wrote)
    }

    /// The known nodes closest to an AssetKey, which the answer tells the asker so the lookup can move toward the key.
    pub async fn closest_nodes(&self, asset_key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        let known = self.node_profile_repo.fetch_node_profiles().await?;
        let my_node_profile = self.my_node_profile.lock().clone();
        Ok(RoutingTable::closest(
            my_node_profile.id(),
            &asset_key.hash.value,
            &known,
            NodeMessage::MAX_CLOSER_NODES,
            my_node_profile.id(),
        ))
    }

    /// Relays an advertisement to the connected nodes closest to the AssetKey, never back to the sender.
    pub async fn relay_locations(&self, locations: HashMap<Arc<AssetKey>, AnnounceLocation>, from: &[u8]) {
        if locations.is_empty() {
            return;
        }

        let statuses = self.sessions.statuses().await;
        // 送信元を除いてから近い順に選ぶ。選んでから除くと、送信元が近傍に入った分だけ配信先が減る
        let ids: Vec<&[u8]> = statuses.iter().map(|(id, _)| id.as_slice()).filter(|id| *id != from).collect();
        let my_node_profile = self.my_node_profile.lock().clone();

        let mut per_session: HashMap<Vec<u8>, HashMap<Arc<AssetKey>, AnnounceLocation>> = HashMap::new();
        for (asset_key, location) in locations {
            for id in RoutingTable::closest_ids(my_node_profile.id(), &asset_key.hash.value, &ids, Self::FAN_OUT) {
                per_session.entry(id.to_vec()).or_default().insert(asset_key.clone(), location.clone());
            }
        }

        for (id, locations) in per_session {
            if let Some((_, status)) = statuses.iter().find(|(session_id, _)| session_id == &id) {
                let locations = AnnounceLocationsBudget::select(locations);
                if let Err(e) = status.outbox.send(NodeMessage::AnnounceLocations { locations }).await {
                    debug!(error_message = e.to_string(), "dropped an advertisement because the outbox is closed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::SeedableRng as _;
    use testresult::TestResult;
    use tokio::sync::Mutex as TokioMutex;
    use tokio_util::bytes::Bytes;

    use omnius_core_base::{clock::ClockUtc, sleeper::SleeperImpl};
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::connection::FramedStream,
        core::session::model::{Session, SessionHandshakeType, SessionType},
        model::{AssetKey, NodeProfile},
        protocol::node::MAX_FRAME_LENGTH,
    };

    use super::*;

    const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

    fn node_profile() -> NodeProfile {
        NodeProfile::random_for_test(vec![OmniAddr::new("addr")])
    }

    fn asset_key(value: u8) -> Arc<AssetKey> {
        Arc::new(AssetKey {
            typ: "test".to_string(),
            hash: omnius_core_omnikit::generated::omni_hash::OmniHash {
                typ: omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType::Sha3_256,
                value: vec![value; 32],
            },
        })
    }

    async fn create_task_communicator(state_dir: &str, intervals: NodeFinderIntervals) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let (session_sender, session_receiver) = mpsc::channel(20);
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(rand::rngs::ChaCha20Rng::from_rng(&mut rand_core::UnwrapErr(rand::rngs::SysRng))));
        let session_connector = Arc::new(SessionConnector::new(
            Arc::new(UnusedTcpConnector),
            signer,
            clock.clone(),
            rng,
            crate::core::session::model::SessionOption::default(),
        ));
        let task = TaskCommunicator::new(
            Arc::new(Mutex::new(node_profile())),
            Arc::new(SessionRegistry::new()),
            session_connector,
            Arc::new(NodeFinderRepo::new(state_dir, clock.clone()).await?),
            Arc::new(Mutex::new(LocationCache::new(clock.clone()))),
            Arc::new(LookupRegistry::new()),
            Arc::new(TokioMutex::new(session_receiver)),
            clock,
            Arc::new(SleeperImpl),
            NodeFinderOption {
                state_dir: state_dir.to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals,
            },
        )
        .await?;
        Ok((task, session_sender))
    }

    struct UnusedTcpConnector;

    #[async_trait::async_trait]
    impl crate::base::connection::ConnectionTcpConnector for UnusedTcpConnector {
        async fn connect(&self, _addr: &OmniAddr) -> Result<tokio::net::TcpStream> {
            Err(Error::new(ErrorKind::NotConnected).with_message("the test does not dial"))
        }
    }

    fn session_pair() -> Result<(Session, FramedStream, NodeProfile)> {
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "peer")?;
        let peer_cert = signer.sign(b"test")?;
        let peer_node_profile = NodeProfile::sign(&signer, vec![], Utc::now())?;
        let (local, peer) = tokio::io::duplex(64 * 1024);
        let (local_reader, local_writer) = tokio::io::split(local);
        let (peer_reader, peer_writer) = tokio::io::split(peer);
        let session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::create_tcp("127.0.0.1".parse()?, 1),
            handshake_type: SessionHandshakeType::Connected,
            cert: peer_cert,
            stream: FramedStream::new(local_reader, local_writer, MAX_FRAME_LENGTH),
        };
        Ok((session, FramedStream::new(peer_reader, peer_writer, MAX_FRAME_LENGTH), peer_node_profile))
    }

    async fn answer_handshake(peer: FramedStream, node_profile: NodeProfile) -> Result<()> {
        let hello: NodeMessage = peer.receiver.lock().await.recv_message_with::<NodeMessageCodec>().await?;
        assert!(matches!(hello, NodeMessage::Hello { .. }));
        peer.sender
            .lock()
            .await
            .send_message_with::<NodeMessageCodec>(&NodeMessage::Hello {
                version: make_bitflags!(NodeFinderVersion::V1),
            })
            .await?;
        let profile: NodeMessage = peer.receiver.lock().await.recv_message_with::<NodeMessageCodec>().await?;
        assert!(matches!(profile, NodeMessage::Profile { .. }));
        peer.sender
            .lock()
            .await
            .send_message_with::<NodeMessageCodec>(&NodeMessage::Profile { node_profile })
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_idle_session_stays_open_through_status() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_millis(200),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;

        let status: NodeMessage = tokio::time::timeout(Duration::from_secs(1), peer.receiver.lock().await.recv_message_with::<NodeMessageCodec>()).await??;
        assert_eq!(status, NodeMessage::Status);

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_silent_peer_closes_after_the_receive_deadline() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_millis(200),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;

        // Status は届くが、相手からは何も送らないので受信期限で閉じる
        let closed = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match peer.receiver.lock().await.recv().await {
                    Ok(_) => continue,
                    Err(e) => {
                        assert_eq!(e.kind(), &omnius_core_omnikit::ErrorKind::EndOfStream);
                        break;
                    }
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "the silent Session is closed by the receive deadline");

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_lookup_answer_can_be_sent_outside_the_cycle() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;

        // 周期が 30 秒でも、outbox に積んだ広告はすぐ送られる
        let statuses = wait_for_session(&task).await?;
        let announcement = NodeMessage::AnnounceProfiles {
            node_profiles: vec![Arc::new(node_profile())],
        };
        statuses.outbox.send(announcement.clone()).await?;
        let received: NodeMessage = tokio::time::timeout(Duration::from_secs(1), peer.receiver.lock().await.recv_message_with::<NodeMessageCodec>()).await??;
        assert_eq!(received, announcement);

        task.shutdown().await;
        Ok(())
    }

    async fn wait_for_session(task: &Arc<TaskCommunicator>) -> Result<Arc<SessionStatus>> {
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            loop {
                if let Some((_, status)) = task.sessions.statuses().await.into_iter().next() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))
    }

    #[tokio::test]
    async fn a_relayed_announcement_carries_one_hop_less_to_the_other_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;

        // 送信元と中継先の 2 Session を確立する
        let (session_a, peer_a, profile_a) = session_pair()?;
        let (session_b, peer_b, profile_b) = session_pair()?;
        let answer_a = answer_handshake(peer_a.clone(), profile_a);
        let answer_b = answer_handshake(peer_b.clone(), profile_b);
        session_sender.send(SessionStatus::new(session_a)).await?;
        session_sender.send(SessionStatus::new(session_b)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            answer_a.await?;
            answer_b.await?;
            while task.sessions.statuses().await.len() != 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<(), Error>(())
        })
        .await??;

        let asset_key = asset_key(1);
        peer_a
            .sender
            .lock()
            .await
            .send_message_with::<NodeMessageCodec>(&NodeMessage::AnnounceLocations {
                locations: HashMap::from([(
                    asset_key.clone(),
                    AnnounceLocation {
                        hops: MAX_HOPS,
                        node_profiles: vec![Arc::new(node_profile())],
                    },
                )]),
            })
            .await?;

        // 中継先は 1 つ減った hop で受け取り、0 になったらそれ以上中継しない
        let relayed: NodeMessage = tokio::time::timeout(Duration::from_secs(2), peer_b.receiver.lock().await.recv_message_with::<NodeMessageCodec>()).await??;
        let NodeMessage::AnnounceLocations { locations } = relayed else {
            panic!("the other Session receives the relayed announcement");
        };
        assert_eq!(locations[&asset_key].hops, MAX_HOPS - 1);

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_zero_hop_announcement_reaches_no_other_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;

        let (session_a, peer_a, profile_a) = session_pair()?;
        let (session_b, peer_b, profile_b) = session_pair()?;
        let answer_a = answer_handshake(peer_a.clone(), profile_a);
        let answer_b = answer_handshake(peer_b.clone(), profile_b);
        session_sender.send(SessionStatus::new(session_a)).await?;
        session_sender.send(SessionStatus::new(session_b)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            answer_a.await?;
            answer_b.await?;
            while task.sessions.statuses().await.len() != 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<(), Error>(())
        })
        .await??;

        let asset_key = asset_key(1);
        peer_a
            .sender
            .lock()
            .await
            .send_message_with::<NodeMessageCodec>(&NodeMessage::AnnounceLocations {
                locations: HashMap::from([(
                    asset_key.clone(),
                    AnnounceLocation {
                        hops: 0,
                        node_profiles: vec![Arc::new(node_profile())],
                    },
                )]),
            })
            .await?;

        // hop 0 は cache に入るだけで、他の Session へは中継されない
        let leaked = tokio::time::timeout(Duration::from_millis(300), peer_b.receiver.lock().await.recv_message_with::<NodeMessageCodec>()).await;
        assert!(leaked.is_err(), "a zero-hop announcement is not relayed");
        assert_eq!(task.location_cache.lock().get(&asset_key).len(), 1);

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn a_profile_announcement_is_not_relayed() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;

        let (session_a, peer_a, profile_a) = session_pair()?;
        let (session_b, peer_b, profile_b) = session_pair()?;
        let answer_a = answer_handshake(peer_a.clone(), profile_a);
        let answer_b = answer_handshake(peer_b.clone(), profile_b);
        session_sender.send(SessionStatus::new(session_a)).await?;
        session_sender.send(SessionStatus::new(session_b)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            answer_a.await?;
            answer_b.await?;
            while task.sessions.statuses().await.len() != 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<(), Error>(())
        })
        .await??;

        peer_a
            .sender
            .lock()
            .await
            .send_message_with::<NodeMessageCodec>(&NodeMessage::AnnounceProfiles {
                node_profiles: vec![Arc::new(node_profile())],
            })
            .await?;

        // 送信元以外の Session には何も届かない
        let leaked = tokio::time::timeout(Duration::from_millis(300), peer_b.receiver.lock().await.recv_message_with::<NodeMessageCodec>()).await;
        assert!(leaked.is_err(), "a profile announcement is not relayed to another Session");

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn an_announcement_is_relayed_with_one_less_hop_and_stops_at_zero() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;
        let status = wait_for_session(&task).await?;

        // 自分が sender しかいないので、hop が 1 なら中継先が無い。hop 0 なら中継もしない。
        let key = asset_key(1);
        let announcement = NodeMessage::AnnounceLocations {
            locations: HashMap::from([(
                key.clone(),
                AnnounceLocation {
                    hops: 0,
                    node_profiles: vec![Arc::new(node_profile())],
                },
            )]),
        };
        peer.sender.lock().await.send_message_with::<NodeMessageCodec>(&announcement).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;

        let locations = task.location_cache.lock().get(&key).to_vec();
        assert_eq!(locations.len(), 1, "a zero-hop advertisement is kept but not relayed");
        assert!(status.outbox.capacity() > 0);

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn the_hop_limit_is_enforced_on_receive() -> TestResult {
        let dir = tempfile::tempdir()?;
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_secs(30),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session)).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;

        // 上限を超える hop は wire の段階で拒否され、Session が閉じる
        let bytes = crate::generated::axus::node::NodeMessage::AnnounceLocations {
            locations: [(
                (*asset_key(1)).clone(),
                crate::generated::axus::node::LocationEntry {
                    hops: MAX_HOPS + 1,
                    node_profiles: vec![],
                },
            )]
            .into_iter()
            .collect(),
        }
        .export()?;
        peer.sender.lock().await.send(Bytes::from(bytes)).await?;

        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match peer.receiver.lock().await.recv().await {
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        })
        .await?;

        task.shutdown().await;
        Ok(())
    }
}

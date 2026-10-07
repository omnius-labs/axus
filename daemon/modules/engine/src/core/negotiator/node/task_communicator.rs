use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use enumflags2::{BitFlags, make_bitflags};
use parking_lot::Mutex;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, mpsc},
    task::JoinHandle,
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;

use omnius_core_base::sleeper::Sleeper;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::session::model::Session,
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskCommunicator {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<SessionRegistry>,
    node_profile_repo: Arc<NodeFinderRepo>,
    session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
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

        for join_handle in self.communicate_join_handles.lock().await.drain(..) {
            let _ = join_handle.await;
        }
    }
}

impl TaskCommunicator {
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<SessionRegistry>,
        node_profile_repo: Arc<NodeFinderRepo>,
        session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let cancellation_token = CancellationToken::new();

        let v = Arc::new(Self {
            my_node_profile,
            sessions,
            node_profile_repo,
            session_receiver,
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
        let my_node_profile = self.my_node_profile.lock().clone();
        // 登録前なので、cancel されたら後始末なしで終える
        let (other_node_profile, received_at) = select! {
            res = Self::handshake(&status.session, &my_node_profile, self.option.intervals.receive_timeout()) => res?,
            _ = self.cancellation_token.cancelled() => return Ok(()),
        };

        *status.node_profile.lock() = Some(other_node_profile.clone());

        let status = Arc::new(status);

        match self.sessions.register(my_node_profile.id(), other_node_profile.id(), &status).await {
            Registration::Added => {}
            Registration::Replaced => info!(node_profile = other_node_profile.to_string(), "Session replaced"),
            Registration::Rejected => return Err(Error::new(ErrorKind::AlreadyExists).with_message("Session already exists")),
        }

        info!(node_profile = other_node_profile.to_string(), "Session established");

        let s = self.clone().send(status.clone()).await;
        let r = self.clone().receive(status.clone(), received_at).await;
        let _ = tokio::join!(s, r);

        info!(node_profile = other_node_profile.to_string(), "Session closed");

        self.sessions.unregister(other_node_profile.id(), &status).await;

        Ok(())
    }

    pub async fn handshake(session: &Session, node_profile: &NodeProfile, receive_timeout: Duration) -> Result<(NodeProfile, Instant)> {
        let deadline = Instant::now() + receive_timeout;
        let send_hello_message = HelloMessage {
            version: make_bitflags!(NodeFinderVersion::V1),
        };
        let (received_hello_message, received_at) = Self::exchange_message(session, &send_hello_message, deadline).await?;

        let version = send_hello_message.version & received_hello_message.version;

        if version.contains(NodeFinderVersion::V1) {
            let send_profile_message = ProfileMessage {
                node_profile: node_profile.clone(),
            };
            let (received_profile_message, received_at) = Self::exchange_message(session, &send_profile_message, received_at + receive_timeout).await?;

            if received_profile_message.node_profile.id() == node_profile.id() {
                return Err(Error::new(ErrorKind::Reject).with_message("connected to self"));
            }

            // node ID は公開鍵から導出するため、Session で署名を確かめた鍵と一致すれば ID も相手のものと確定する
            if received_profile_message.node_profile.public_key() != session.cert.public_key.as_slice() {
                return Err(Error::new(ErrorKind::Reject).with_message("node profile does not match the session certificate"));
            }

            Ok((received_profile_message.node_profile, received_at))
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("invalid version: {}", version.bits())))
        }
    }

    async fn exchange_message<T: RocketPackStruct + Send + Sync>(session: &Session, message: &T, deadline: Instant) -> Result<(T, Instant)> {
        let received = timeout_at(deadline, async {
            session.stream.sender.lock().await.send_message(message).await?;
            session.stream.receiver.lock().await.recv_message::<T>().await
        })
        .await
        .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
        Ok((received, Instant::now()))
    }

    async fn send(self: Arc<Self>, status: Arc<SessionStatus>) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let sender = TaskSender { status };
            let f = async {
                loop {
                    this.sleeper.sleep(this.option.intervals.communicate).await;
                    let res = sender.send().await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "send failed",);
                        return;
                    }
                }
            };
            select! {
                _ = f => {}
                _ = this.cancellation_token.cancelled() => {}
                _ = sender.status.cancellation_token.cancelled() => {}
            };
            sender.status.cancellation_token.cancel();
        })
    }

    async fn receive(self: Arc<Self>, status: Arc<SessionStatus>, received_at: Instant) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let receiver = TaskReceiver {
                status,
                node_profile_repo: this.node_profile_repo.clone(),
            };
            let f = async {
                let communicate = this.option.intervals.communicate;
                let receive_timeout = this.option.intervals.receive_timeout();
                let mut deadline = received_at + receive_timeout;
                loop {
                    let res = async {
                        let data_message = timeout_at(deadline, receiver.receive())
                            .await
                            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
                        // 受信した時点で更新し、保存処理にかかった時間で期限を延ばさない
                        let received_at = Instant::now();
                        deadline = received_at + receive_timeout;
                        timeout_at(deadline, receiver.apply(data_message))
                            .await
                            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError).with_message("NodeFinder receive timed out"))??;
                        // 周期より速く送る相手に、保存処理を繰り返させない
                        this.sleeper.sleep(communicate.saturating_sub(received_at.elapsed())).await;
                        Ok::<(), Error>(())
                    }
                    .await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "receive failed",);
                        return;
                    }
                }
            };
            select! {
                _ = f => {}
                _ = this.cancellation_token.cancelled() => {}
                _ = receiver.status.cancellation_token.cancelled() => {}
            }
            receiver.status.cancellation_token.cancel();
        })
    }
}

struct TaskSender {
    status: Arc<SessionStatus>,
}

impl TaskSender {
    async fn send(&self) -> Result<()> {
        let data_message = {
            let mut sending_data_message = self.status.sending_data_message.lock();
            DataMessage {
                push_node_profiles: sending_data_message.push_node_profiles.drain(..).collect(),
                want_asset_keys: sending_data_message.want_asset_keys.drain(..).collect(),
                give_asset_key_locations: sending_data_message.give_asset_key_locations.drain().collect(),
                push_asset_key_locations: sending_data_message.push_asset_key_locations.drain().collect(),
            }
        };

        self.status.session.stream.sender.lock().await.send_message(&data_message).await?;

        Ok(())
    }
}

struct TaskReceiver {
    status: Arc<SessionStatus>,
    node_profile_repo: Arc<NodeFinderRepo>,
}

impl TaskReceiver {
    async fn receive(&self) -> Result<DataMessage> {
        self.status.session.stream.receiver.lock().await.recv_message().await
    }

    async fn apply(&self, data_message: DataMessage) -> Result<()> {
        let push_node_profiles: Vec<&NodeProfile> = data_message.push_node_profiles.iter().take(32).map(|n| n.as_ref()).collect();
        self.node_profile_repo.insert_or_ignore_node_profiles(&push_node_profiles, 0).await?;
        self.node_profile_repo.shrink(1024).await?;

        {
            let mut received_data_message = self.status.received_data_message.lock();
            received_data_message.want_asset_keys.extend(data_message.want_asset_keys);
            received_data_message.give_asset_key_locations.extend(data_message.give_asset_key_locations);
            received_data_message.push_asset_key_locations.extend(data_message.push_asset_key_locations);

            received_data_message.want_asset_keys.shrink(1024 * 256);
            received_data_message.give_asset_key_locations.shrink(1024 * 256);
            received_data_message.push_asset_key_locations.shrink(1024 * 256);
        }

        Ok(())
    }
}

#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
enum NodeFinderVersion {
    V1 = 1,
}

#[derive(Debug, PartialEq, Eq)]
struct HelloMessage {
    pub version: BitFlags<NodeFinderVersion>,
}

impl RocketPackStruct for HelloMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.version.bits())?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut version: Option<BitFlags<NodeFinderVersion>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => version = Some(BitFlags::<NodeFinderVersion>::from_bits_truncate(decoder.read_u32()?)),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            version: version.ok_or(RocketPackDecoderError::Other("missing field: version"))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ProfileMessage {
    pub node_profile: NodeProfile,
}

impl RocketPackStruct for ProfileMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_struct(&value.node_profile)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut node_profile: Option<NodeProfile> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => node_profile = Some(decoder.read_struct::<NodeProfile>()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            node_profile: node_profile.ok_or(RocketPackDecoderError::Other("missing field: node_profile"))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DataMessage {
    pub push_node_profiles: Vec<Arc<NodeProfile>>,
    pub want_asset_keys: Vec<Arc<AssetKey>>,
    pub give_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
    pub push_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
}

impl DataMessage {
    pub fn new() -> Self {
        Self {
            push_node_profiles: vec![],
            want_asset_keys: vec![],
            give_asset_key_locations: HashMap::new(),
            push_asset_key_locations: HashMap::new(),
        }
    }
}

impl Default for DataMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl RocketPackStruct for DataMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(4)?;

        encoder.write_u64(0)?;
        encoder.write_array(value.push_node_profiles.len())?;
        for profile in value.push_node_profiles.iter() {
            encoder.write_struct(profile.as_ref())?;
        }

        encoder.write_u64(1)?;
        encoder.write_array(value.want_asset_keys.len())?;
        for asset_key in value.want_asset_keys.iter() {
            encoder.write_struct(asset_key.as_ref())?;
        }

        encoder.write_u64(2)?;
        encoder.write_map(value.give_asset_key_locations.len())?;
        for (asset_key, profiles) in value.give_asset_key_locations.iter() {
            encoder.write_struct(asset_key.as_ref())?;
            encoder.write_array(profiles.len())?;
            for profile in profiles.iter() {
                encoder.write_struct(profile.as_ref())?;
            }
        }

        encoder.write_u64(3)?;
        encoder.write_map(value.push_asset_key_locations.len())?;
        for (asset_key, profiles) in value.push_asset_key_locations.iter() {
            encoder.write_struct(asset_key.as_ref())?;
            encoder.write_array(profiles.len())?;
            for profile in profiles.iter() {
                encoder.write_struct(profile.as_ref())?;
            }
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut push_node_profiles: Option<Vec<Arc<NodeProfile>>> = None;
        let mut want_asset_keys: Option<Vec<Arc<AssetKey>>> = None;
        let mut give_asset_key_locations: Option<HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = None;
        let mut push_asset_key_locations: Option<HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let count = decoder.read_array()?;
                    let mut profiles = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                    }
                    push_node_profiles = Some(profiles);
                }
                1 => {
                    let count = decoder.read_array()?;
                    let mut asset_keys = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        asset_keys.push(Arc::new(decoder.read_struct::<AssetKey>()?));
                    }
                    want_asset_keys = Some(asset_keys);
                }
                2 => {
                    let count = decoder.read_map()?;
                    let mut map = HashMap::with_capacity(count as usize);
                    for _ in 0..count {
                        let key = Arc::new(decoder.read_struct::<AssetKey>()?);
                        let count = decoder.read_array()?;
                        let mut profiles = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                        }
                        map.insert(key, profiles);
                    }
                    give_asset_key_locations = Some(map);
                }
                3 => {
                    let count = decoder.read_map()?;
                    let mut map = HashMap::with_capacity(count as usize);
                    for _ in 0..count {
                        let key = Arc::new(decoder.read_struct::<AssetKey>()?);
                        let count = decoder.read_array()?;
                        let mut profiles = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                        }
                        map.insert(key, profiles);
                    }
                    push_asset_key_locations = Some(map);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            push_node_profiles: push_node_profiles.ok_or(RocketPackDecoderError::Other("missing field: push_node_profiles"))?,
            want_asset_keys: want_asset_keys.ok_or(RocketPackDecoderError::Other("missing field: want_asset_keys"))?,
            give_asset_key_locations: give_asset_key_locations.ok_or(RocketPackDecoderError::Other("missing field: give_asset_key_locations"))?,
            push_asset_key_locations: push_asset_key_locations.ok_or(RocketPackDecoderError::Other("missing field: push_asset_key_locations"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use chrono::Utc;
    use enumflags2::{BitFlags, make_bitflags};
    use parking_lot::Mutex;
    use testresult::TestResult;
    use tokio::{
        sync::{Mutex as TokioMutex, mpsc},
        time::Instant,
    };
    use tokio_util::bytes::Bytes;

    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::{Sleeper, SleeperImpl},
    };
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::{
            connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
            runtime::Shutdown as _,
        },
        core::session::model::{Session, SessionHandshakeType, SessionType},
        model::NodeProfile,
        prelude::*,
    };

    use super::{
        DataMessage, HelloMessage, NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, NodeFinderVersion, ProfileMessage, SessionRegistry, SessionStatus, TaskCommunicator,
    };

    const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn shutdown_completes_while_waiting_for_a_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (task, _session_sender) = create_task_communicator(dir.path().to_str().unwrap(), Arc::new(SessionRegistry::new())).await?;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn shutdown_completes_during_handshake() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), sessions.clone()).await?;

        // 相手が応答しないので、handshake の待機に入ったまま止まる
        let (session, _peer, _) = session_pair()?;
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());
        assert_eq!(sessions.len().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn shutdown_removes_the_registered_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator(dir.path().to_str().unwrap(), sessions.clone()).await?;

        // peer を保持し続けることで、Session は相手の切断では終わらず shutdown まで残る
        let (session, peer, peer_node_profile) = session_pair()?;
        let peer_task = tokio::spawn(answer_handshake(peer.clone(), peer_node_profile));
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        peer_task.await??;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while sessions.len().await == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        assert!(tokio::time::timeout(SHUTDOWN_TIMEOUT, task.shutdown()).await.is_ok());
        assert_eq!(sessions.len().await, 0);
        drop(peer);

        Ok(())
    }

    #[tokio::test]
    async fn oversized_node_finder_frame_closes_both_workers_and_removes_the_session() -> TestResult {
        let dir = tempfile::tempdir()?;
        let sessions = Arc::new(SessionRegistry::new());
        let intervals = NodeFinderIntervals {
            communicate: Duration::from_millis(10),
            ..NodeFinderIntervals::default()
        };
        let (task, session_sender) = create_task_communicator_with_intervals(dir.path().to_str().unwrap(), sessions.clone(), intervals).await?;
        let (session, peer, peer_node_profile) = session_pair()?;
        session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        let answer = answer_handshake(peer.clone(), peer_node_profile);
        session_sender.send(SessionStatus::new(session, Arc::new(ClockUtc))).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, answer).await??;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            while sessions.len().await == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        // 送信側だけが動き続けないことを、相手からの EOF と登録解除で確かめる
        let send = async { peer.sender.lock().await.send(Bytes::from(vec![0; FramedStream::NODE_FINDER_MAX_FRAME_LENGTH + 1])).await };
        let receive = async {
            loop {
                if peer.receiver.lock().await.recv().await.is_err() {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, async { tokio::join!(send, receive) }).await?;
        assert_eq!(sessions.len().await, 0);
        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn silent_session_closes_after_three_communicate_intervals() -> TestResult {
        for interval in [Duration::from_millis(50), Duration::from_millis(100)] {
            let dir = tempfile::tempdir()?;
            let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
            fixture.answer_handshake().await?;
            let start = Instant::now();
            fixture.wait_registered().await?;
            let count = fixture.wait_closed(interval * 4).await?;
            assert!(start.elapsed() >= interval * 2);
            // 受信がなくても、期限が来るまでは空の DataMessage を毎周期送る
            assert!(count >= 2);
            fixture.task.shutdown().await;
        }
        Ok(())
    }

    #[tokio::test]
    async fn data_received_before_the_deadline_keeps_the_session_open_and_resets_the_timeout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(50);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        fixture.answer_handshake().await?;
        fixture.wait_registered().await?;

        // 3 周期より長く交換し、受信のたびに期限が更新されることを確かめる
        for _ in 0..6 {
            fixture.peer.sender.lock().await.send_message(&DataMessage::default()).await?;
            let received: DataMessage = tokio::time::timeout(interval * 2, fixture.peer.receiver.lock().await.recv_message()).await??;
            assert_eq!(received, DataMessage::default());
            assert_eq!(fixture.sessions.len().await, 1);
        }
        fixture.wait_closed(interval * 4).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn receive_waits_for_the_rest_of_the_interval_after_each_message() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(400);
        let intervals = NodeFinderIntervals {
            communicate: interval,
            ..NodeFinderIntervals::default()
        };
        let sleeper = Arc::new(RecordingSleeper {
            requested: Mutex::new(Vec::new()),
        });
        let sessions = Arc::new(SessionRegistry::new());
        let (task, session_sender) = create_task_communicator_with_sleeper(dir.path().to_str().unwrap(), sessions.clone(), intervals, sleeper.clone()).await?;
        let (session, peer, node_profile) = session_pair()?;
        session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
        session_sender
            .send(SessionStatus::new(session, Arc::new(ClockUtc)))
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
        answer_handshake(peer.clone(), node_profile).await?;

        peer.sender.lock().await.send_message(&DataMessage::default()).await?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
            // 送信側は周期ちょうどを、受信側は周期から処理にかかった分を引いた時間を要求する
            while !sleeper.requested.lock().iter().any(|d| *d > interval / 2 && *d < interval) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;

        task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn silent_hello_exchange_closes_on_the_receive_deadline() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(50);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
        fixture.wait_closed(interval * 4).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn profile_exchange_has_a_receive_deadline_that_is_reset_by_hello() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(100);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
        tokio::time::sleep(interval * 2).await;
        fixture.send_hello().await?;
        let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;

        // Hello の受信で更新されるので、handshake 開始から 3 周期を過ぎても閉じない
        assert!(tokio::time::timeout(interval * 3 / 2, fixture.peer.receiver.lock().await.recv()).await.is_err());
        fixture.wait_closed(interval * 2).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn profile_received_before_the_deadline_establishes_the_session_and_resets_the_timeout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let interval = Duration::from_millis(100);
        let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), interval).await?;
        fixture.send_hello().await?;
        let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
        let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
        tokio::time::sleep(interval * 2).await;
        fixture
            .peer
            .sender
            .lock()
            .await
            .send_message(&ProfileMessage {
                node_profile: fixture.node_profile.clone(),
            })
            .await?;
        fixture.wait_registered().await?;

        // Profile の受信前の期限を引き継ぐと、2 周期目の DataMessage を受け取れない
        for _ in 0..2 {
            let received: DataMessage = tokio::time::timeout(interval * 2, fixture.peer.receiver.lock().await.recv_message()).await??;
            assert_eq!(received, DataMessage::default());
        }
        fixture.wait_closed(interval * 2).await?;
        fixture.task.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_cancels_receive_deadlines_in_hello_profile_and_established_phases() -> TestResult {
        for phase in 0..3 {
            let dir = tempfile::tempdir()?;
            let fixture = CommunicatingSession::new(dir.path().to_str().unwrap(), Duration::from_secs(3600)).await?;
            let _: HelloMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
            if phase >= 1 {
                fixture.send_hello().await?;
                let _: ProfileMessage = tokio::time::timeout(SHUTDOWN_TIMEOUT, fixture.peer.receiver.lock().await.recv_message()).await??;
            }
            if phase == 2 {
                fixture
                    .peer
                    .sender
                    .lock()
                    .await
                    .send_message(&ProfileMessage {
                        node_profile: fixture.node_profile.clone(),
                    })
                    .await?;
                fixture.wait_registered().await?;
            }
            tokio::time::timeout(Duration::from_millis(300), fixture.task.shutdown()).await?;
            fixture.wait_closed(Duration::from_millis(300)).await?;
        }
        Ok(())
    }

    struct CommunicatingSession {
        task: Arc<TaskCommunicator>,
        sessions: Arc<SessionRegistry>,
        _session_sender: mpsc::Sender<SessionStatus>,
        peer: FramedStream,
        node_profile: NodeProfile,
    }

    impl CommunicatingSession {
        async fn new(state_dir: &str, interval: Duration) -> Result<Self> {
            let sessions = Arc::new(SessionRegistry::new());
            let intervals = NodeFinderIntervals {
                communicate: interval,
                ..NodeFinderIntervals::default()
            };
            let (task, session_sender) = create_task_communicator_with_intervals(state_dir, sessions.clone(), intervals).await?;
            let (session, peer, node_profile) = session_pair()?;
            session.stream.set_max_frame_length(FramedStream::NODE_FINDER_MAX_FRAME_LENGTH).await;
            session_sender
                .send(SessionStatus::new(session, Arc::new(ClockUtc)))
                .await
                .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
            Ok(Self {
                task,
                sessions,
                _session_sender: session_sender,
                peer,
                node_profile,
            })
        }

        async fn answer_handshake(&self) -> Result<()> {
            answer_handshake(self.peer.clone(), self.node_profile.clone()).await
        }

        async fn send_hello(&self) -> Result<()> {
            self.peer
                .sender
                .lock()
                .await
                .send_message(&HelloMessage {
                    version: make_bitflags!(NodeFinderVersion::V1),
                })
                .await
        }

        async fn wait_registered(&self) -> Result<()> {
            tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
                while self.sessions.len().await == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?;
            Ok(())
        }

        async fn wait_closed(&self, timeout: Duration) -> Result<usize> {
            let count = tokio::time::timeout(timeout, async {
                let mut count = 0;
                loop {
                    match self.peer.receiver.lock().await.recv().await {
                        Ok(frame) => {
                            assert_eq!(DataMessage::import(&frame)?, DataMessage::default());
                            count += 1;
                        }
                        Err(e) => {
                            assert_eq!(e.kind(), &omnius_core_omnikit::ErrorKind::EndOfStream);
                            break;
                        }
                    }
                }
                Result::Ok(count)
            })
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))??;
            assert_eq!(self.sessions.len().await, 0);
            Ok(count)
        }
    }

    #[tokio::test]
    async fn handshake_succeeds_with_common_version() -> TestResult {
        let (session, peer, peer_node_profile) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, peer_node_profile.clone()));

        let (received, _) = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout()).await?;
        assert_eq!(received, peer_node_profile);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_peer_without_common_version() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(async move {
            peer.sender.lock().await.send_message(&HelloMessage { version: BitFlags::empty() }).await?;
            let _: HelloMessage = peer.receiver.lock().await.recv_message().await?;
            Result::Ok(())
        });

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::UnsupportedType);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_peer_with_own_id() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("me")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_profile_that_does_not_match_the_certificate() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("someone else")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me"), NodeFinderIntervals::default().receive_timeout())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    async fn create_task_communicator(state_dir: &str, sessions: Arc<SessionRegistry>) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        create_task_communicator_with_intervals(state_dir, sessions, NodeFinderIntervals::default()).await
    }

    async fn create_task_communicator_with_intervals(
        state_dir: &str,
        sessions: Arc<SessionRegistry>,
        intervals: NodeFinderIntervals,
    ) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        create_task_communicator_with_sleeper(state_dir, sessions, intervals, Arc::new(SleeperImpl)).await
    }

    async fn create_task_communicator_with_sleeper(
        state_dir: &str,
        sessions: Arc<SessionRegistry>,
        intervals: NodeFinderIntervals,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Result<(Arc<TaskCommunicator>, mpsc::Sender<SessionStatus>)> {
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let (session_sender, session_receiver) = mpsc::channel(20);

        let task = TaskCommunicator::new(
            Arc::new(Mutex::new(node_profile("me"))),
            sessions,
            Arc::new(NodeFinderRepo::new(state_dir, clock).await?),
            Arc::new(TokioMutex::new(session_receiver)),
            sleeper,
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

    /// 要求された待ち時間を記録して、実際に待つ Sleeper。
    struct RecordingSleeper {
        requested: Mutex<Vec<Duration>>,
    }

    #[async_trait]
    impl Sleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.requested.lock().push(duration);
            tokio::time::sleep(duration).await;
        }
    }

    async fn answer_handshake(peer: FramedStream, node_profile: NodeProfile) -> Result<()> {
        peer.sender
            .lock()
            .await
            .send_message(&HelloMessage {
                version: make_bitflags!(NodeFinderVersion::V1),
            })
            .await?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message().await?;
        peer.sender.lock().await.send_message(&ProfileMessage { node_profile }).await?;
        let _: ProfileMessage = peer.receiver.lock().await.recv_message().await?;
        Ok(())
    }

    /// local 側の Session と、相手側の stream および Session の cert と一致する相手の NodeProfile を返す
    fn session_pair() -> Result<(Session, FramedStream, NodeProfile)> {
        let (local, peer) = tokio::io::duplex(64 * 1024);
        let (local_reader, local_writer) = tokio::io::split(local);
        let (peer_reader, peer_writer) = tokio::io::split(peer);

        let peer_signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let peer_cert = peer_signer.sign(b"test")?;
        let peer_node_profile = NodeProfile::new(peer_cert.public_key.clone(), vec![]);
        let session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::create_tcp("127.0.0.1".parse()?, 1),
            handshake_type: SessionHandshakeType::Connected,
            cert: peer_cert,
            stream: FramedStream::new(local_reader, local_writer),
        };

        Ok((session, FramedStream::new(peer_reader, peer_writer), peer_node_profile))
    }

    fn node_profile(public_key: &str) -> NodeProfile {
        NodeProfile::new(public_key.as_bytes().to_vec(), vec![])
    }
}

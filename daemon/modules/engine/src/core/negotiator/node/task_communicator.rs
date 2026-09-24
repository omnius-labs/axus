use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use enumflags2::{BitFlags, make_bitflags};
use futures::FutureExt;
use parking_lot::Mutex;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use omnius_core_base::sleeper::Sleeper;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::session::model::{Session, SessionHandshakeType},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskCommunicator {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
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
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }

        self.cancellation_token.cancel();

        for join_handle in self.communicate_join_handles.lock().await.drain(..) {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

impl TaskCommunicator {
    pub async fn new(
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
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
            loop {
                // 終了済みのタスクを削除
                this.communicate_join_handles.lock().await.retain(|join_handle| !join_handle.is_finished());

                if let Some(status) = this.session_receiver.lock().await.recv().await {
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
        let other_node_profile = Self::handshake(&status.session, &my_node_profile).await?;

        *status.node_profile.lock() = Some(other_node_profile.clone());

        let status = Arc::new(status);

        let replaced = {
            let mut sessions = self.sessions.write().await;
            if let Some(existing) = sessions.get(other_node_profile.id())
                && Self::keeps_existing_session(
                    my_node_profile.id(),
                    other_node_profile.id(),
                    &existing.session.handshake_type,
                    &status.session.handshake_type,
                )
            {
                return Err(Error::new(ErrorKind::AlreadyExists).with_message("Session already exists"));
            }
            sessions.insert(other_node_profile.id().to_vec(), status.clone())
        };

        if let Some(replaced) = replaced {
            replaced.cancellation_token.cancel();
            info!(node_profile = other_node_profile.to_string(), "Session replaced");
        }

        info!(node_profile = other_node_profile.to_string(), "Session established");

        let s = self.clone().send(status.clone()).await;
        let r = self.clone().receive(status.clone()).await;
        let _ = tokio::join!(s, r);

        info!(node_profile = other_node_profile.to_string(), "Session closed");

        // 入れ替えで後から登録された Session を消さないよう、自分が登録した Session のときだけ消す
        {
            let mut sessions = self.sessions.write().await;
            if sessions.get(other_node_profile.id()).is_some_and(|current| Arc::ptr_eq(current, &status)) {
                sessions.remove(other_node_profile.id());
            }
        }

        Ok(())
    }

    /// 同じ相手との Session が重複したとき、既存の Session を残すかどうかを返す。
    /// node ID が小さい側から張った Session を残す規則にすると、両側が追加の message なしに同じ Session を選ぶ。
    fn keeps_existing_session(my_id: &[u8], other_id: &[u8], existing: &SessionHandshakeType, new: &SessionHandshakeType) -> bool {
        let preferred = if my_id < other_id {
            SessionHandshakeType::Connected
        } else {
            SessionHandshakeType::Accepted
        };
        *existing == preferred || *new != preferred
    }

    pub async fn handshake(session: &Session, node_profile: &NodeProfile) -> Result<NodeProfile> {
        let send_hello_message = HelloMessage {
            version: make_bitflags!(NodeFinderVersion::V1),
        };
        session.stream.sender.lock().await.send_message(&send_hello_message).await?;
        let received_hello_message: HelloMessage = session.stream.receiver.lock().await.recv_message().await?;

        let version = send_hello_message.version & received_hello_message.version;

        if version.contains(NodeFinderVersion::V1) {
            let send_profile_message = ProfileMessage {
                node_profile: node_profile.clone(),
            };
            session.stream.sender.lock().await.send_message(&send_profile_message).await?;
            let received_profile_message: ProfileMessage = session.stream.receiver.lock().await.recv_message().await?;

            if received_profile_message.node_profile.id() == node_profile.id() {
                return Err(Error::new(ErrorKind::Reject).with_message("connected to self"));
            }

            // node ID は公開鍵から導出するため、Session で署名を確かめた鍵と一致すれば ID も相手のものと確定する
            if received_profile_message.node_profile.public_key() != session.cert.public_key.as_slice() {
                return Err(Error::new(ErrorKind::Reject).with_message("node profile does not match the session certificate"));
            }

            Ok(received_profile_message.node_profile)
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("invalid version: {}", version.bits())))
        }
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
        })
    }

    async fn receive(self: Arc<Self>, status: Arc<SessionStatus>) -> JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let receiver = TaskReceiver {
                status,
                node_profile_repo: this.node_profile_repo.clone(),
            };
            let f = async {
                loop {
                    this.sleeper.sleep(this.option.intervals.communicate).await;
                    let res = receiver.receive().await;
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
    async fn receive(&self) -> Result<()> {
        let data_message = self.status.session.stream.receiver.lock().await.recv_message::<DataMessage>().await?;

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
    use enumflags2::{BitFlags, make_bitflags};
    use testresult::TestResult;

    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
        core::session::model::{Session, SessionHandshakeType, SessionType},
        model::NodeProfile,
        prelude::*,
    };

    use super::{HelloMessage, NodeFinderVersion, ProfileMessage, TaskCommunicator};

    #[tokio::test]
    async fn handshake_succeeds_with_common_version() -> TestResult {
        let (session, peer, peer_node_profile) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, peer_node_profile.clone()));

        let received = TaskCommunicator::handshake(&session, &node_profile("me")).await?;
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

        let err = TaskCommunicator::handshake(&session, &node_profile("me")).await.unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::UnsupportedType);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_peer_with_own_id() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("me")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me")).await.unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    #[tokio::test]
    async fn handshake_rejects_profile_that_does_not_match_the_certificate() -> TestResult {
        let (session, peer, _) = session_pair()?;

        let peer_task = tokio::spawn(answer_handshake(peer, node_profile("someone else")));

        let err = TaskCommunicator::handshake(&session, &node_profile("me")).await.unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::Reject);
        peer_task.await??;

        Ok(())
    }

    #[test]
    fn both_nodes_keep_the_session_dialed_by_the_smaller_id() {
        use SessionHandshakeType::{Accepted, Connected};

        let small: &[u8] = &[1];
        let large: &[u8] = &[2];

        // 小さい ID の側が張った接続を X、大きい ID の側が張った接続を Y とすると、
        // 小さい側では X が Connected、Y が Accepted に、大きい側では X が Accepted、Y が Connected に見える
        for (my_id, other_id, x, y) in [(small, large, Connected, Accepted), (large, small, Accepted, Connected)] {
            // X が先に登録された場合は X を残し、Y が先に登録された場合は X に入れ替える
            assert!(TaskCommunicator::keeps_existing_session(my_id, other_id, &x, &y));
            assert!(!TaskCommunicator::keeps_existing_session(my_id, other_id, &y, &x));
        }

        // 同じ方向の接続が重複した場合は、既存の Session を残す
        assert!(TaskCommunicator::keeps_existing_session(small, large, &Accepted, &Accepted));
        assert!(TaskCommunicator::keeps_existing_session(large, small, &Connected, &Connected));
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

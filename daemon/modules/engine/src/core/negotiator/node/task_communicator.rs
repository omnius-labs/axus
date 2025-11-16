use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use enumflags2::BitFlags;
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
        Shutdown,
        connection::{FramedRecvExt as _, FramedSendExt as _},
    },
    core::session::model::Session,
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
    #[allow(unused)]
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

        {
            let mut sessions = self.sessions.write().await;
            if sessions.contains_key(&other_node_profile.id) {
                return Err(Error::new(ErrorKind::AlreadyExists).with_message("Session already exists"));
            }
            sessions.insert(other_node_profile.id.clone(), status.clone());
        }

        info!(node_profile = other_node_profile.to_string(), "Session established");

        let s = self.clone().send(status.clone()).await;
        let r = self.clone().receive(status.clone()).await;
        let _ = tokio::join!(s, r);

        info!(node_profile = other_node_profile.to_string(), "Session closed");

        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&other_node_profile.id);
        }

        Ok(())
    }

    pub async fn handshake(session: &Session, node_profile: &NodeProfile) -> Result<NodeProfile> {
        let send_hello_message = HelloMessage { version: NodeFinderVersion::V1 };
        session.stream.sender.lock().await.send_message(&send_hello_message).await?;
        let received_hello_message: HelloMessage = session.stream.receiver.lock().await.recv_message().await?;

        let version = send_hello_message.version | received_hello_message.version;

        if version.contains(NodeFinderVersion::V1) {
            let send_profile_message = ProfileMessage {
                node_profile: node_profile.clone(),
            };
            session.stream.sender.lock().await.send_message(&send_profile_message).await?;
            let received_profile_message: ProfileMessage = session.stream.receiver.lock().await.recv_message().await?;

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
                    this.sleeper.sleep(std::time::Duration::from_secs(20)).await;
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
                    this.sleeper.sleep(std::time::Duration::from_secs(20)).await;
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
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
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
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
    }
}

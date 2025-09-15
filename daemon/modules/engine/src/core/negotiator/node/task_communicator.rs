use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use bitflags::bitflags;
use futures::FutureExt;
use parking_lot::Mutex;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use omnius_core_base::{ensure_err, sleeper::Sleeper};

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
                return Err(Error::builder().kind(ErrorKind::AlreadyExists).message("Session already exists").build());
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
            Err(Error::builder().kind(ErrorKind::UnsupportedVersion).message("Invalid version").build())
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

bitflags! {
    #[derive(Debug, PartialEq, Eq )]
      struct NodeFinderVersion: u32 {
        const V1 = 1;
    }
}

#[derive(Debug, PartialEq, Eq)]
struct HelloMessage {
    pub version: NodeFinderVersion,
}

impl RocketMessage for HelloMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(value.version.bits());

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let version = NodeFinderVersion::from_bits(reader.get_u32()?)
            .ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).message("invalid version").build())?;

        Ok(Self { version })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ProfileMessage {
    pub node_profile: NodeProfile,
}

impl RocketMessage for ProfileMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        NodeProfile::pack(writer, &value.node_profile, depth + 1)?;

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let node_profile = NodeProfile::unpack(reader, depth + 1)?;

        Ok(Self { node_profile })
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

impl RocketMessage for DataMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        writer.put_u32(value.push_node_profiles.len() as u32);
        for v in &value.push_node_profiles {
            NodeProfile::pack(writer, v, depth + 1)?;
        }

        writer.put_u32(value.want_asset_keys.len() as u32);
        for v in &value.want_asset_keys {
            AssetKey::pack(writer, v, depth + 1)?;
        }

        writer.put_u32(value.give_asset_key_locations.len() as u32);
        for (key, vs) in &value.give_asset_key_locations {
            AssetKey::pack(writer, key, depth + 1)?;
            writer.put_u32(vs.len() as u32);
            for v in vs {
                NodeProfile::pack(writer, v, depth + 1)?;
            }
        }

        writer.put_u32(value.push_asset_key_locations.len() as u32);
        for (key, vs) in &value.push_asset_key_locations {
            AssetKey::pack(writer, key, depth + 1)?;
            writer.put_u32(vs.len() as u32);
            for v in vs {
                NodeProfile::pack(writer, v, depth + 1)?;
            }
        }

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let get_too_large_err = || RocketPackError::builder().kind(RocketPackErrorKind::TooLarge).message("len too large").build();

        let len = reader.get_u32()? as usize;
        ensure_err!(len > 128, get_too_large_err);

        let mut push_node_profiles = Vec::with_capacity(len);
        for _ in 0..len {
            push_node_profiles.push(Arc::new(NodeProfile::unpack(reader, depth + 1)?));
        }

        let len = reader.get_u32()? as usize;
        ensure_err!(len > 128, get_too_large_err);

        let mut want_asset_keys = Vec::with_capacity(len);
        for _ in 0..len {
            want_asset_keys.push(Arc::new(AssetKey::unpack(reader, depth + 1)?));
        }

        let len = reader.get_u32()? as usize;
        ensure_err!(len > 128, get_too_large_err);

        let mut give_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>> = HashMap::new();
        for _ in 0..len {
            let key = Arc::new(AssetKey::unpack(reader, depth + 1)?);
            let len = reader.get_u32()? as usize;
            ensure_err!(len > 128, get_too_large_err);

            let mut vs = Vec::with_capacity(len);
            for _ in 0..len {
                vs.push(Arc::new(NodeProfile::unpack(reader, depth + 1)?));
            }
            give_asset_key_locations.entry(key).or_default().extend(vs);
        }

        let len = reader.get_u32()? as usize;
        ensure_err!(len > 128, get_too_large_err);

        let mut push_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>> = HashMap::new();
        for _ in 0..len {
            let key = Arc::new(AssetKey::unpack(reader, depth + 1)?);
            let len = reader.get_u32()? as usize;
            ensure_err!(len > 128, get_too_large_err);

            let mut vs = Vec::with_capacity(len);
            for _ in 0..len {
                vs.push(Arc::new(NodeProfile::unpack(reader, depth + 1)?));
            }
            push_asset_key_locations.entry(key).or_default().extend(vs);
        }

        Ok(Self {
            push_node_profiles,
            want_asset_keys,
            give_asset_key_locations,
            push_asset_key_locations,
        })
    }
}

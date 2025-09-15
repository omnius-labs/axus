use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt;
use parking_lot::Mutex;
use rand::{
    SeedableRng,
    seq::{IndexedRandom as _, SliceRandom},
};
use rand_chacha::ChaCha20Rng;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};
use omnius_core_omnikit::model::OmniHash;

use crate::{
    base::{Shutdown, collections::VolatileHashSet},
    core::{
        negotiator::NodeFinder,
        session::{
            SessionConnector,
            model::{SessionHandshakeType, SessionType},
        },
    },
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskConnector {
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    session_connector: Arc<SessionConnector>,
    node_finder: Arc<NodeFinder>,
    file_publisher: Arc<TokioMutex<Option<Arc<FilePublisher>>>>,
    file_subscriber: Arc<TokioMutex<Option<Arc<FileSubscriber>>>>,
    connected_node_profiles: Arc<Mutex<VolatileHashSet<Arc<NodeProfile>>>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: FileExchangerOption,
    join_handles: Arc<TokioMutex<Vec<JoinHandle<()>>>>,
}

#[async_trait]
impl Shutdown for TaskConnector {
    async fn shutdown(&self) {
        for join_handle in self.join_handles.lock().await.drain(..) {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

impl TaskConnector {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
        session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
        session_connector: Arc<SessionConnector>,
        node_finder: Arc<NodeFinder>,
        file_publisher: Arc<TokioMutex<Option<Arc<FilePublisher>>>>,
        file_subscriber: Arc<TokioMutex<Option<Arc<FileSubscriber>>>>,
        connected_node_profiles: Arc<Mutex<VolatileHashSet<Arc<NodeProfile>>>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: FileExchangerOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            sessions,
            session_sender,
            session_connector,
            node_finder,
            file_publisher,
            file_subscriber,
            connected_node_profiles,
            sleeper,
            clock,
            option,
            join_handles: Arc::new(TokioMutex::new(vec![])),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        {
            let this = self.clone();
            let join_handle = tokio::spawn(async move {
                loop {
                    this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                    let res = this.connect_for_publish().await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "connect failed");
                    }
                }
            });
            self.join_handles.lock().await.push(join_handle);
        }

        {
            let this = self.clone();
            let join_handle = tokio::spawn(async move {
                loop {
                    this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                    let res = this.connect_for_subscribe().await;
                    if let Err(e) = res {
                        warn!(error_message = e.to_string(), "connect failed");
                    }
                }
            });
            self.join_handles.lock().await.push(join_handle);
        }

        Ok(())
    }

    async fn connect_for_publish(&self) -> Result<()> {
        let session_count = self
            .sessions
            .read()
            .await
            .iter()
            .filter(|(_, status)| status.session.handshake_type == SessionHandshakeType::Connected && status.exchange_type == ExchangeType::Publish)
            .count();
        if session_count >= self.option.max_connected_session_for_publish_count {
            return Ok(());
        }

        let root_hashes = {
            if let Some(file_publisher) = self.file_publisher.lock().await.as_ref() {
                file_publisher.as_ref().get_published_root_hashes().await?
            } else {
                return Ok(());
            }
        };
        self.connect_sub(ExchangeType::Publish, root_hashes).await
    }

    async fn connect_for_subscribe(&self) -> Result<()> {
        let session_count = self
            .sessions
            .read()
            .await
            .iter()
            .filter(|(_, status)| status.session.handshake_type == SessionHandshakeType::Connected && status.exchange_type == ExchangeType::Subscribe)
            .count();
        if session_count >= self.option.max_connected_session_for_subscribe_count {
            return Ok(());
        }

        let root_hashes = {
            if let Some(file_subscriber) = self.file_subscriber.lock().await.as_ref() {
                file_subscriber.as_ref().get_subscribed_root_hashes().await?
            } else {
                return Ok(());
            }
        };
        self.connect_sub(ExchangeType::Subscribe, root_hashes).await
    }

    async fn connect_sub(&self, exchange_type: ExchangeType, root_hashes: Vec<OmniHash>) -> Result<()> {
        self.connected_node_profiles.lock().refresh();

        let connected_ids: HashSet<Vec<u8>> = {
            let v1: Vec<Vec<u8>> = self.connected_node_profiles.lock().iter().map(|n| n.id.to_owned()).collect();
            let v2: Vec<Vec<u8>> = self.sessions.read().await.iter().map(|n| n.0.to_owned()).collect();
            v1.into_iter().chain(v2.into_iter()).collect()
        };

        let mut asset_keys: Vec<AssetKey> = root_hashes.into_iter().map(|hash| AssetKey { typ: "file".to_string(), hash }).collect();

        let mut rng = ChaCha20Rng::from_os_rng();
        asset_keys.shuffle(&mut rng);

        for asset_key in asset_keys {
            let node_profiles: Vec<Arc<NodeProfile>> = self
                .node_finder
                .find_node_profile(&asset_key)
                .await?
                .into_iter()
                .filter(|n| !connected_ids.contains(&n.id))
                .collect();
            let Some(node_profile) = node_profiles.choose(&mut rng) else {
                continue;
            };

            for addr in node_profile.addrs.iter() {
                if let Ok(session) = self.session_connector.connect(addr, &SessionType::FileExchanger).await {
                    let status = SessionStatus::new(exchange_type, session, Some(asset_key.hash.clone()), self.clock.clone());
                    self.session_sender
                        .lock()
                        .await
                        .send(status)
                        .await
                        .map_err(|e| Error::builder().kind(ErrorKind::UnexpectedError).source(e).build())?;
                    self.connected_node_profiles.lock().insert(node_profile.clone());

                    return Ok(());
                }
            }
        }

        Ok(())
    }
}

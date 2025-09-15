use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt;
use parking_lot::Mutex;
use rand::{SeedableRng, seq::IndexedRandom as _};
use rand_chacha::ChaCha20Rng;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::{Shutdown, collections::VolatileHashSet},
    core::session::{
        SessionConnector,
        model::{SessionHandshakeType, SessionType},
    },
    model::NodeProfile,
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskConnector {
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    session_connector: Arc<SessionConnector>,
    connected_node_profiles: Arc<Mutex<VolatileHashSet<NodeProfile>>>,
    node_profile_repo: Arc<NodeFinderRepo>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl Shutdown for TaskConnector {
    async fn shutdown(&self) {
        if let Some(join_handle) = self.join_handle.lock().await.take() {
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
        connected_node_profiles: Arc<Mutex<VolatileHashSet<NodeProfile>>>,
        node_profile_repo: Arc<NodeFinderRepo>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            sessions,
            session_sender,
            session_connector,
            connected_node_profiles,
            node_profile_repo,
            clock,
            sleeper,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            loop {
                this.sleeper.sleep(std::time::Duration::from_secs(20)).await;
                let res = this.connect().await;
                if let Err(e) = res {
                    warn!(error_message = e.to_string(), "connect failed");
                }
            }
        }));

        Ok(())
    }

    async fn connect(&self) -> Result<()> {
        let session_count = self
            .sessions
            .read()
            .await
            .iter()
            .filter(|(_, status)| status.session.handshake_type == SessionHandshakeType::Connected)
            .count();
        if session_count >= self.option.max_connected_session_count {
            return Ok(());
        }

        self.connected_node_profiles.lock().refresh();

        let connected_ids: HashSet<Vec<u8>> = {
            let v1: Vec<Vec<u8>> = self.connected_node_profiles.lock().iter().map(|n| n.id.to_owned()).collect();
            let v2: Vec<Vec<u8>> = self.sessions.read().await.iter().map(|n| n.0.to_owned()).collect();
            v1.into_iter().chain(v2.into_iter()).collect()
        };

        let node_profiles: Vec<NodeProfile> = self
            .node_profile_repo
            .fetch_node_profiles()
            .await?
            .into_iter()
            .filter(|n| !connected_ids.contains(&n.id))
            .collect();

        let mut rng = ChaCha20Rng::from_os_rng();
        let node_profile = node_profiles
            .choose(&mut rng)
            .ok_or_else(|| Error::builder().kind(ErrorKind::NotFound).message("node profile is not found").build())?;

        for addr in node_profile.addrs.iter() {
            if let Ok(session) = self.session_connector.connect(addr, &SessionType::NodeFinder).await {
                let status = SessionStatus::new(session, self.clock.clone());
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

        Ok(())
    }
}

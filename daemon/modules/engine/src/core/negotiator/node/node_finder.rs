use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::future::join_all;
use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc};

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::{
        collections::VolatileHashSet,
        runtime::Shutdown,
        sync::{FnHandle, FnHub},
    },
    core::session::{SessionAccepter, SessionConnector},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[allow(dead_code)]
pub struct NodeFinder {
    my_node_profile: Arc<Mutex<NodeProfile>>,
    session_connector: Arc<SessionConnector>,
    session_accepter: Arc<SessionAccepter>,
    node_profile_repo: Arc<NodeFinderRepo>,
    node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    option: NodeFinderOption,

    session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    connected_node_profiles: Arc<Mutex<VolatileHashSet<NodeProfile>>>,
    connecting_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
    get_want_asset_keys_fn: Arc<FnHub<Vec<AssetKey>, ()>>,
    get_push_asset_keys_fn: Arc<FnHub<Vec<AssetKey>, ()>>,

    task_connectors: Arc<TokioMutex<Vec<Arc<TaskConnector>>>>,
    task_acceptors: Arc<TokioMutex<Vec<Arc<TaskAccepter>>>>,
    task_computer: Arc<TokioMutex<Option<Arc<TaskComputer>>>>,
    task_communicator: Arc<TokioMutex<Option<Arc<TaskCommunicator>>>>,
}

#[derive(Debug, Clone)]
pub struct NodeFinderOption {
    #[allow(unused)]
    pub state_dir: String,
    pub max_connected_session_count: usize,
    pub max_accepted_session_count: usize,
    pub intervals: NodeFinderIntervals,
}

#[derive(Debug, Clone)]
pub struct NodeFinderIntervals {
    pub connect: std::time::Duration,
    pub compute: std::time::Duration,
    pub communicate: std::time::Duration,
}

impl Default for NodeFinderIntervals {
    fn default() -> Self {
        Self {
            connect: std::time::Duration::from_secs(20),
            compute: std::time::Duration::from_secs(60),
            communicate: std::time::Duration::from_secs(20),
        }
    }
}

impl NodeFinder {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        my_node_profile: NodeProfile,
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        node_profile_repo: Arc<NodeFinderRepo>,
        node_profile_fetcher: Arc<dyn NodeProfileFetcher + Send + Sync>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: NodeFinderOption,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel(20);

        let v = Self {
            my_node_profile: Arc::new(Mutex::new(my_node_profile)),
            session_connector,
            session_accepter,
            node_profile_repo,
            node_profile_fetcher,
            clock: clock.clone(),
            sleeper,
            rng,
            option,

            session_receiver: Arc::new(TokioMutex::new(rx)),
            session_sender: Arc::new(TokioMutex::new(tx)),
            sessions: Arc::new(TokioRwLock::new(HashMap::new())),
            connected_node_profiles: Arc::new(Mutex::new(VolatileHashSet::new(Duration::seconds(180), clock))),
            connecting_ids: Arc::new(Mutex::new(HashSet::new())),
            get_want_asset_keys_fn: Arc::new(FnHub::new()),
            get_push_asset_keys_fn: Arc::new(FnHub::new()),

            task_connectors: Arc::new(TokioMutex::new(Vec::new())),
            task_acceptors: Arc::new(TokioMutex::new(Vec::new())),
            task_computer: Arc::new(TokioMutex::new(None)),
            task_communicator: Arc::new(TokioMutex::new(None)),
        };
        v.start().await?;

        Ok(v)
    }

    #[allow(unused)]
    pub async fn get_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    #[allow(unused)]
    pub fn my_node_profile(&self) -> NodeProfile {
        self.my_node_profile.lock().clone()
    }

    #[allow(unused)]
    pub fn listen_want_asset_keys<F>(&self, f: F) -> FnHandle<Vec<AssetKey>, ()>
    where
        F: Fn(&()) -> Vec<AssetKey> + Send + Sync + 'static,
    {
        self.get_want_asset_keys_fn.listener().listen(f)
    }

    #[allow(unused)]
    pub fn listen_push_asset_keys<F>(&self, f: F) -> FnHandle<Vec<AssetKey>, ()>
    where
        F: Fn(&()) -> Vec<AssetKey> + Send + Sync + 'static,
    {
        self.get_push_asset_keys_fn.listener().listen(f)
    }

    async fn start(&self) -> Result<()> {
        for _ in 0..3 {
            let task = TaskConnector::new(
                self.my_node_profile.clone(),
                self.sessions.clone(),
                self.session_sender.clone(),
                self.session_connector.clone(),
                self.connected_node_profiles.clone(),
                self.connecting_ids.clone(),
                self.node_profile_repo.clone(),
                self.clock.clone(),
                self.sleeper.clone(),
                self.rng.clone(),
                self.option.clone(),
            )
            .await?;
            self.task_connectors.lock().await.push(task);
        }

        for _ in 0..3 {
            let task = TaskAccepter::new(
                self.sessions.clone(),
                self.session_sender.clone(),
                self.session_accepter.clone(),
                self.clock.clone(),
                self.sleeper.clone(),
                self.option.clone(),
            )
            .await?;
            self.task_acceptors.lock().await.push(task);
        }

        let task = TaskComputer::new(
            self.my_node_profile.clone(),
            self.node_profile_repo.clone(),
            self.node_profile_fetcher.clone(),
            self.sessions.clone(),
            self.get_want_asset_keys_fn.caller(),
            self.get_push_asset_keys_fn.caller(),
            self.sleeper.clone(),
            self.rng.clone(),
            self.option.clone(),
        )
        .await?;
        self.task_computer.lock().await.replace(task);

        let task = TaskCommunicator::new(
            self.my_node_profile.clone(),
            self.sessions.clone(),
            self.node_profile_repo.clone(),
            self.session_receiver.clone(),
            self.sleeper.clone(),
            self.option.clone(),
        )
        .await?;
        self.task_communicator.lock().await.replace(task);

        Ok(())
    }

    pub async fn find_node_profile(&self, key: &AssetKey) -> Result<Vec<Arc<NodeProfile>>> {
        let mut results: Vec<Arc<NodeProfile>> = Vec::new();

        let sessions = self.sessions.read().await;
        for status in sessions.values() {
            let received_data_message = status.received_data_message.lock();
            if let Some(node_profiles) = received_data_message.give_asset_key_locations.get(key) {
                results.extend(node_profiles.clone());
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Shutdown for NodeFinder {
    async fn shutdown(&self) {
        {
            let mut task_connectors = self.task_connectors.lock().await;
            let task_connectors: Vec<Arc<TaskConnector>> = task_connectors.drain(..).collect();
            join_all(task_connectors.iter().map(|task| task.shutdown())).await;
        }

        {
            let mut task_acceptors = self.task_acceptors.lock().await;
            let task_acceptors: Vec<Arc<TaskAccepter>> = task_acceptors.drain(..).collect();
            join_all(task_acceptors.iter().map(|task| task.shutdown())).await;
        }

        {
            let mut task_computer = self.task_computer.lock().await;
            if let Some(task_computer) = task_computer.take() {
                task_computer.shutdown().await;
            }
        }

        {
            let mut task_communicator = self.task_communicator.lock().await;
            if let Some(task_communicator) = task_communicator.take() {
                task_communicator.shutdown().await;
            }
        }

        self.session_accepter.shutdown().await;
    }
}

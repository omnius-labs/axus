use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt;
use parking_lot::Mutex;
use rand::seq::IndexedRandom;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::{collections::VolatileHashSet, runtime::Shutdown},
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
    my_node_profile: Arc<Mutex<NodeProfile>>,
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    session_connector: Arc<SessionConnector>,
    connected_node_profiles: Arc<Mutex<VolatileHashSet<NodeProfile>>>,
    connecting_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
    node_profile_repo: Arc<NodeFinderRepo>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
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
        my_node_profile: Arc<Mutex<NodeProfile>>,
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
        session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
        session_connector: Arc<SessionConnector>,
        connected_node_profiles: Arc<Mutex<VolatileHashSet<NodeProfile>>>,
        connecting_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
        node_profile_repo: Arc<NodeFinderRepo>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            my_node_profile,
            sessions,
            session_sender,
            session_connector,
            connected_node_profiles,
            connecting_ids,
            node_profile_repo,
            clock,
            sleeper,
            option,
            rng,
            join_handle: Arc::new(TokioMutex::new(None)),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            loop {
                this.sleeper.sleep(this.option.intervals.connect).await;
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

        let excluded_ids: HashSet<Vec<u8>> = {
            let v1: Vec<Vec<u8>> = self.connected_node_profiles.lock().iter().map(|n| n.id.to_owned()).collect();
            let v2: Vec<Vec<u8>> = self.sessions.read().await.iter().map(|n| n.0.to_owned()).collect();
            let my_id = self.my_node_profile.lock().id.clone();
            v1.into_iter().chain(v2).chain([my_id]).collect()
        };

        let node_profiles: Vec<NodeProfile> = self
            .node_profile_repo
            .fetch_node_profiles()
            .await?
            .into_iter()
            .filter(|n| !excluded_ids.contains(&n.id))
            .collect();

        // 同じ node の TaskConnector が同じ相手へ同時に接続しないよう、相手の選択と予約を 1 つの lock の中で行う
        let node_profile = {
            let mut connecting_ids = self.connecting_ids.lock();
            let candidates: Vec<&NodeProfile> = node_profiles.iter().filter(|n| !connecting_ids.contains(&n.id)).collect();
            let node_profile = {
                let mut rng = self.rng.lock();
                (*candidates
                    .choose(&mut *rng)
                    .ok_or_else(|| Error::new(ErrorKind::NotFound).with_message("node profile is not found"))?)
                .clone()
            };
            connecting_ids.insert(node_profile.id.clone());
            node_profile
        };

        let result = self.connect_node(&node_profile).await;
        self.connecting_ids.lock().remove(&node_profile.id);
        result
    }

    async fn connect_node(&self, node_profile: &NodeProfile) -> Result<()> {
        for addr in node_profile.addrs.iter() {
            if let Ok(session) = self.session_connector.connect(addr, &SessionType::NodeFinder).await {
                let status = SessionStatus::new(session, self.clock.clone());
                self.session_sender
                    .lock()
                    .await
                    .send(status)
                    .await
                    .map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;

                self.connected_node_profiles.lock().insert(node_profile.clone());

                return Ok(());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use chrono::Utc;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;
    use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc};

    use omnius_core_base::{
        clock::{Clock, ClockUtc},
        sleeper::SleeperImpl,
    };
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::{
            collections::VolatileHashSet,
            connection::{ConnectionTcpConnector, FramedStream},
            runtime::Shutdown as _,
        },
        core::session::SessionConnector,
        model::NodeProfile,
        prelude::*,
    };

    use super::{NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, TaskConnector};

    #[tokio::test]
    async fn connect_skips_own_node_profile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let my_node_profile = node_profile("me", 1);
        let tcp_connector = Arc::new(CountingTcpConnector::default());
        let task = create_task_connector(
            dir.path().to_str().unwrap(),
            &my_node_profile,
            &[&my_node_profile],
            tcp_connector.clone(),
            Arc::new(Mutex::new(HashSet::new())),
        )
        .await?;

        let err = task.connect().await.unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::NotFound);
        assert_eq!(tcp_connector.count(), 0);

        task.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn concurrent_connects_do_not_target_the_same_node() -> TestResult {
        let dir = tempfile::tempdir()?;
        let my_node_profile = node_profile("me", 1);
        let other_node_profile = node_profile("other", 2);
        let tcp_connector = Arc::new(CountingTcpConnector::default());
        let connecting_ids = Arc::new(Mutex::new(HashSet::new()));

        let mut tasks = Vec::new();
        for i in 0..3 {
            let state_dir = dir.path().join(i.to_string());
            std::fs::create_dir_all(&state_dir)?;
            let task = create_task_connector(
                state_dir.to_str().unwrap(),
                &my_node_profile,
                &[&other_node_profile],
                tcp_connector.clone(),
                connecting_ids.clone(),
            )
            .await?;
            tasks.push(task);
        }

        let results = futures::future::join_all(tasks.iter().map(|task| task.connect())).await;
        let not_found_count = results.iter().filter(|result| matches!(result, Err(e) if e.kind() == &ErrorKind::NotFound)).count();
        assert_eq!(tcp_connector.count(), 1);
        assert_eq!(not_found_count, 2);
        assert!(connecting_ids.lock().is_empty());

        for task in tasks {
            task.shutdown().await;
        }

        Ok(())
    }

    async fn create_task_connector(
        state_dir: &str,
        my_node_profile: &NodeProfile,
        node_profiles: &[&NodeProfile],
        tcp_connector: Arc<CountingTcpConnector>,
        connecting_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
    ) -> Result<Arc<TaskConnector>> {
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);

        let node_profile_repo = Arc::new(NodeFinderRepo::new(state_dir, clock.clone()).await?);
        node_profile_repo.insert_or_ignore_node_profiles(node_profiles, 0).await?;

        let (session_sender, _session_receiver) = mpsc::channel(20);

        TaskConnector::new(
            Arc::new(Mutex::new(my_node_profile.clone())),
            Arc::new(TokioRwLock::new(HashMap::new())),
            Arc::new(TokioMutex::new(session_sender)),
            Arc::new(SessionConnector::new(tcp_connector, signer, rng.clone())),
            Arc::new(Mutex::new(VolatileHashSet::new(chrono::Duration::seconds(180), clock.clone()))),
            connecting_ids,
            node_profile_repo,
            clock,
            Arc::new(SleeperImpl),
            rng,
            NodeFinderOption {
                state_dir: state_dir.to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals: NodeFinderIntervals::default(),
            },
        )
        .await
    }

    fn node_profile(id: &str, port: u16) -> NodeProfile {
        NodeProfile {
            id: id.as_bytes().to_vec(),
            addrs: vec![OmniAddr::create_tcp("127.0.0.1".parse().unwrap(), port)],
        }
    }

    #[derive(Default)]
    struct CountingTcpConnector {
        count: Mutex<usize>,
    }

    impl CountingTcpConnector {
        fn count(&self) -> usize {
            *self.count.lock()
        }
    }

    #[async_trait]
    impl ConnectionTcpConnector for CountingTcpConnector {
        async fn connect(&self, _addr: &OmniAddr) -> Result<FramedStream> {
            *self.count.lock() += 1;
            tokio::time::sleep(Duration::from_millis(100)).await;
            Err(Error::new(ErrorKind::NotConnected).with_message("counting connector does not connect"))
        }
    }
}
